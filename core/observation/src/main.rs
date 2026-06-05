use dotenvy::dotenv;
use futures::{SinkExt, StreamExt};
use std::{collections::HashMap, env};
use tracing::{error, info, warn};
use tracing_subscriber::FmtSubscriber;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterSlots, SubscribeRequestPing,
    subscribe_update::UpdateOneof,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    // Init structuredd logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let endpoint = env::var("GRPC_ENDPOINT")?;
    let token = env::var("GRPC_X_TOKEN")?;

    info!(endpoint = %endpoint, "Connecting to Yellowstone");

    let tls_config = ClientTlsConfig::new().with_native_roots();
    let mut client = GeyserGrpcClient::build_from_shared(endpoint)?
        .x_token(Some(token))?
        .tls_config(tls_config)?
        .connect()
        .await?;

    info!("Connected successfully");

    // Build slots subscription request
    let mut slots = HashMap::new();
    slots.insert(
        "slots".to_string(),
        SubscribeRequestFilterSlots {
            filter_by_commitment: None,
            interslot_updates: None,
        },
    );

    let request = SubscribeRequest {
        slots,
        ..Default::default()
    };

    // Open stream
    let (mut sink, mut stream) = client.subscribe_with_request(Some(request)).await?;

    info!("Slot stream open, listening...");

    // Consume stream
    while let Some(message) = stream.next().await {
        match message {
            Ok(msg) => {
                match msg.update_oneof {
                    Some(UpdateOneof::Ping(_)) => {
                        // Reply with pong to keep connection alive
                        sink.send(SubscribeRequest {
                            ping: Some(SubscribeRequestPing { id: 1 }),
                            ..Default::default()
                        })
                        .await?;
                        info!("Ping received, pong sent");
                    }
                    Some(UpdateOneof::Slot(slot)) => {
                        let status = match slot.status {
                            0 => "Processed",
                            1 => "Confirmed",
                            2 => "Finalized",
                            _ => "Unknown",
                        };
                        info!(slot = slot.slot, status = status, "Slot update");
                    }
                    _ => {}
                }
            }
            Err(e) => error!("Error receiving slot update: {:?}", e),
        }
    }

    warn!("Stream ended unexpectedly");
    Ok(())
}

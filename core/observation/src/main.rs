mod events;
mod leader;

use dotenvy::dotenv;
use events::{NetworkEvent, SlotStatus, create_event_channel};
use futures::{SinkExt, StreamExt};
use leader::LeaderSchedule;
use std::{collections::HashMap, env};
use tracing::{error, info, warn};
use tracing_subscriber::FmtSubscriber;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterSlots, SubscribeRequestPing,
    subscribe_update::UpdateOneof,
};

const LEADER_WINDOW: u64 = 4;
const EVENT_CHANNEL_BUFFER: usize = 1000;

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
    let rpc_url = env::var("RPC_ENDPOINT")?;

    // Create event channel
    let (tx, mut rx) = create_event_channel(EVENT_CHANNEL_BUFFER);

    // Spawn event consumer — simulates L2 receiving events
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                NetworkEvent::SlotUpdate { slot, status } => {
                    info!(slot, ?status, "L2 received slot event");
                }
                NetworkEvent::LeaderWindow { slot, leader } => {
                    info!(slot, %leader, "L2 received leader event");
                }
                NetworkEvent::BlockObserved { slot } => {
                    info!(slot, "L2 received block event");
                }
                NetworkEvent::TransactionObserved { signature } => {
                    info!(%signature, "L2 received transaction event");
                }
            }
        }
    });

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

    let mut leader_schedule: Option<LeaderSchedule> = None;

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
                        let status = SlotStatus::from_u32(slot.status as u32);

                        // Emit slot event
                        tx.send(NetworkEvent::SlotUpdate {
                            slot: slot.slot,
                            status: status.clone(),
                        })
                        .await?;

                        // Fetch leader schedule once on first processed slot
                        if leader_schedule.is_none() && matches!(status, SlotStatus::Processed) {
                            match LeaderSchedule::fetch(&rpc_url, slot.slot) {
                                Ok(schedule) => {
                                    info!("Leader schedule ready");
                                    leader_schedule = Some(schedule);
                                }
                                Err(e) => error!(error = %e, "Failed to fetch leader schedule"),
                            }
                        }

                        // Emit leader window events on each processed slot
                        if matches!(status, SlotStatus::Processed) {
                            if let Some(ref schedule) = leader_schedule {
                                let upcoming =
                                    schedule.get_upcoming_leaders(slot.slot, LEADER_WINDOW);
                                for (upcoming_slot, leader) in upcoming {
                                    tx.send(NetworkEvent::LeaderWindow {
                                        slot: upcoming_slot,
                                        leader: leader.to_string(),
                                    })
                                    .await?;
                                }
                            }
                        }
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

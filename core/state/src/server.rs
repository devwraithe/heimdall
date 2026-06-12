use crate::engine::OperationalState;
use crate::heimdall::{
    OperationalSnapshot, SubscribeRequest,
    operational_state_service_server::OperationalStateService,
};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{error, info};

/// How often L5 pushes a snapshot to L6 in milliseconds
const SNAPSHOT_INTERVAL_MS: u64 = 500;

pub struct StateServer {
    state: Arc<Mutex<OperationalState>>,
}

impl StateServer {
    pub fn new(state: Arc<Mutex<OperationalState>>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl OperationalStateService for StateServer {
    type SubscribeStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<OperationalSnapshot, Status>> + Send>>;

    async fn subscribe(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        info!("L6 agent subscribed to operational state stream");

        let state = Arc::clone(&self.state);
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            loop {
                let snapshot = {
                    match state.lock() {
                        Ok(s) => s.snapshot(),
                        Err(e) => {
                            error!(error = %e, "Failed to lock state");
                            break;
                        }
                    }
                };

                if tx.send(Ok(snapshot)).await.is_err() {
                    info!("L6 agent disconnected from state stream");
                    break;
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(SNAPSHOT_INTERVAL_MS)).await;
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

use crate::engine::to_snapshot;
use crate::heimdall::{
    OperationalSnapshot, SubscribeRequest,
    operational_state_service_server::OperationalStateService,
};
use crate::heimdall::{HealthRequest, HealthResponse, RetryRequest, RetryResponse};
use shared::engine::OperationalState;
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
    retry_tx: mpsc::Sender<RetryRequest>,
}

impl StateServer {
    pub fn new(state: Arc<Mutex<OperationalState>>, retry_tx: mpsc::Sender<RetryRequest>) -> Self {
        Self { state, retry_tx }
    }
}

#[tonic::async_trait]
impl OperationalStateService for StateServer {
    type SubscribeStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<OperationalSnapshot, Status>> + Send>>;

    async fn subscribe(
        &self,
        _request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        info!("L6 agent subscribed to operational state stream");

        let state = Arc::clone(&self.state);
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            loop {
                let snapshot = {
                    match state.lock() {
                        Ok(s) => to_snapshot(&*s),
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

    async fn retry(
        &self,
        request: Request<RetryRequest>,
    ) -> Result<Response<RetryResponse>, Status> {
        let retry = request.into_inner();

        info!(
            confidence = retry.confidence,
            risk = %retry.observed_risk,
            classification = %retry.failure_classification,
            "Retry request received via gRPC"
        );

        if self.retry_tx.send(retry).await.is_err() {
            return Err(Status::internal("Failed to queue retry request"));
        }

        Ok(Response::new(RetryResponse {
            accepted: true,
            message: "Retry accepted".into(),
        }))
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        let (healthy, uptime, slot) = match self.state.lock() {
            Ok(s) => (true, s.uptime_seconds(), s.current_slot),
            Err(_) => (false, 0, 0),
        };

        Ok(Response::new(HealthResponse {
            healthy,
            uptime_seconds: uptime,
            current_slot: slot,
        }))
    }
}

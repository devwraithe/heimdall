use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::CommitmentConfig;
use solana_sdk::hash::Hash;
use tracing::info;

/// Controls whether the fetcher returns a real
/// or intentionally expired blockhash
#[derive(Debug, Clone, PartialEq)]
pub enum BlockhashMode {
    /// Fetches a genuine recent blockhash
    Normal,
    /// Returns a zeroed blockhash to simulate expiry
    FaultInjected,
}

pub struct BlockhashFetcher {
    rpc_client: RpcClient,
    mode: BlockhashMode,
}

impl BlockhashFetcher {
    pub fn new(rpc_url: &str, mode: BlockhashMode) -> Self {
        Self {
            rpc_client: RpcClient::new_with_commitment(
                rpc_url.to_string(),
                CommitmentConfig::confirmed(),
            ),
            mode,
        }
    }

    fn fetch_real(&self) -> Result<Hash> {
        let blockhash = self.rpc_client.get_latest_blockhash()?;

        info!(%blockhash, "Fetched fresh blockhash");
        Ok(blockhash)
    }

    /// Fetches a blockhash based on current mode.
    /// FaultInjected mode returns a zeroed hash
    /// to simulate blockhash expiry.
    pub fn fetch(&self) -> Result<Hash> {
        match self.mode {
            BlockhashMode::Normal => self.fetch_real(),
            BlockhashMode::FaultInjected => {
                let expired = Hash::default();
                info!("Fault injection active — returning expired blockhash");
                Ok(expired)
            }
        }
    }

    /// Forces a real blockhash fetch even if fault injection is enabled.
    pub fn fetch_fresh(&self) -> Result<Hash> {
        self.fetch_real()
    }
}

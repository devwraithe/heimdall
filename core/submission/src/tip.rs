use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tracing::info;

// / Jito's 8 tip accounts on mainnet
const JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvB8BoaQmX1DRunAoUnFYzaqAmqn1B7bMA",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

/// Fetches real tip account data and calculates
/// a competitive tip amount in lamports
pub struct TipCalculator {
    rpc_client: RpcClient,
}

impl TipCalculator {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_client: RpcClient::new(rpc_url.to_string()),
        }
    }

    /// Fetches live tip account balances and returns
    /// a competitive tip amount in lamports
    pub fn calculate(&self) -> Result<u64> {
        // Fetch balances of all 8 tip accounts
        let mut balances: Vec<u64> = JITO_TIP_ACCOUNTS
            .iter()
            .filter_map(|addr| {
                Pubkey::from_str(addr)
                    .ok()
                    .and_then(|pubkey| self.rpc_client.get_balance(&pubkey).ok())
            })
            .collect();

        if balances.is_empty() {
            // Fallback minimum if RPC fails entirely
            let fallback = 1_000;
            info!(
                tip_lamports = fallback,
                "No tip data available, using fallback minimum"
            );
            return Ok(fallback);
        }

        // Sort ascending for median calculation
        balances.sort_unstable();

        // Take median balance
        let median = balances[balances.len() / 2];

        // Apply 10% premium over median to stay competitive
        let tip = (median as f64 * 1.1) as u64;

        // Floor at 1000 lamports — never tip zero
        let tip = tip.max(1_000);

        info!(
            median_lamports = median,
            tip_lamports = tip,
            accounts_sampled = balances.len(),
            "Tip calculated from live data"
        );

        Ok(tip)
    }
}

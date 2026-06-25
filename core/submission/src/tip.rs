use anyhow::Result;
use jito_sdk::client::TipClient;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::env;
use std::str::FromStr;
use tracing::{info, warn};

fn tip_premium_percent() -> f64 {
    env::var("TIP_PREMIUM_PERCENT")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(10.0)
}

fn tip_min_lamports() -> u64 {
    env::var("TIP_MIN_LAMPORTS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1_000)
}

const DEFAULT_JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvB8BoaQmX1DRunAoUnFYzaqAmqn1B7bMA",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

fn get_jito_tip_accounts() -> Vec<String> {
    let raw_accounts =
        env::var("JITO_TIP_ACCOUNTS").unwrap_or_else(|_| DEFAULT_JITO_TIP_ACCOUNTS.join(","));

    raw_accounts
        .split(',')
        .map(|s| s.trim().to_string())
        .collect()
}

fn derive_tip_from_balances(balances: &[u64]) -> u64 {
    let min_tip = tip_min_lamports();
    if balances.is_empty() {
        return min_tip;
    }

    let premium = 1.0 + (tip_premium_percent() / 100.0);
    let mut sorted = balances.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    (median as f64 * premium).round() as u64
}

// Fetches real tip account data and calculates a competitive tip amount in lamports
pub struct TipCalculator {
    rpc_client: RpcClient,
    tip_client: TipClient,
}

impl TipCalculator {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_client: RpcClient::new(rpc_url.to_string()),
            tip_client: TipClient::new(),
        }
    }

    fn calculate_from_balances(&self) -> Result<u64> {
        // Fetch balances of all 8 tip accounts
        let balances: Vec<u64> = get_jito_tip_accounts()
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

        let median = balances[balances.len() / 2];
        let tip = derive_tip_from_balances(&balances).max(tip_min_lamports());

        info!(
            median_lamports = median,
            tip_lamports = tip,
            accounts_sampled = balances.len(),
            "Tip calculated from live data"
        );

        Ok(tip)
    }

    // Fetches the current Jito recommended tip when available, then falls back to the median-balance proxy.
    pub async fn calculate(&self) -> Result<u64> {
        let tip = match self.tip_client.get_recommended_tip().await {
            Ok(recommended) if recommended > 0 => {
                info!(
                    tip_lamports = recommended,
                    "Tip fetched from Jito recommended tip API"
                );
                recommended
            }
            Ok(_) => {
                warn!("Jito recommended tip returned zero, falling back to balance proxy");
                self.calculate_from_balances()?
            }
            Err(e) => {
                warn!(error = %e, "Failed to fetch Jito recommended tip, falling back to balance proxy");
                self.calculate_from_balances()?
            }
        };

        Ok(tip.max(tip_min_lamports()))
    }
}

#[cfg(test)]
mod tests {
    use super::derive_tip_from_balances;

    #[test]
    fn derives_tip_from_median_balance() {
        assert_eq!(derive_tip_from_balances(&[1_000, 2_000, 3_000]), 2_200);
    }

    #[test]
    fn tip_has_floor_when_balances_are_empty() {
        assert_eq!(derive_tip_from_balances(&[]), 1_000);
    }
}

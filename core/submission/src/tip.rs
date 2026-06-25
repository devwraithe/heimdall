use anyhow::Result;
use jito_sdk::client::TipClient;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::env;
use std::str::FromStr;
use tracing::{info, warn};

const TIP_FLOOR_URL: &str = "https://bundles.jito.wtf/api/v1/bundles/tip_floor";
const DEFAULT_TIP_MIN_LAMPORTS: u64 = 30_000;
const DEFAULT_TIP_MAX_LAMPORTS: u64 = 100_000;

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
        .unwrap_or(DEFAULT_TIP_MIN_LAMPORTS)
}

fn tip_max_lamports() -> u64 {
    env::var("TIP_MAX_LAMPORTS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIP_MAX_LAMPORTS)
}

fn clamp_tip(tip: u64) -> u64 {
    tip.clamp(tip_min_lamports(), tip_max_lamports())
}

const DEFAULT_JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvB8BoaQmX1DRunAoUnFYzaqAmqn1B7bMA",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee5pbDmJGcLWNDXjh",
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
    clamp_tip((median as f64 * premium).round() as u64)
}

async fn fetch_tip_floor_p75_lamports() -> Option<u64> {
    let response = reqwest::get(TIP_FLOOR_URL).await.ok()?;
    let payload: serde_json::Value = response.json().await.ok()?;
    let sol = payload[0]["landed_tips_75th_percentile"]
        .as_f64()
        .or_else(|| payload["landed_tips_75th_percentile"].as_f64())?;

    if sol <= 0.0 {
        return None;
    }

    Some(clamp_tip((sol * 1_000_000_000.0).round() as u64))
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
        let balances: Vec<u64> = get_jito_tip_accounts()
            .iter()
            .filter_map(|addr| {
                Pubkey::from_str(addr)
                    .ok()
                    .and_then(|pubkey| self.rpc_client.get_balance(&pubkey).ok())
            })
            .collect();

        if balances.is_empty() {
            let fallback = tip_min_lamports();
            info!(
                tip_lamports = fallback,
                "No tip data available, using fallback minimum"
            );
            return Ok(fallback);
        }

        let tip = derive_tip_from_balances(&balances);

        info!(
            tip_lamports = tip,
            accounts_sampled = balances.len(),
            "Tip calculated from live tip-account balances (clamped)"
        );

        Ok(tip)
    }

    pub async fn calculate(&self) -> Result<u64> {
        if let Some(tip) = fetch_tip_floor_p75_lamports().await {
            info!(
                tip_lamports = tip,
                "Tip fetched from Jito tip floor API (p75, clamped)"
            );
            return Ok(tip);
        }

        let tip = match self.tip_client.get_recommended_tip().await {
            Ok(recommended) if recommended > 0 => {
                let clamped = clamp_tip(recommended);
                info!(
                    tip_lamports = clamped,
                    raw_recommended = recommended,
                    "Tip fetched from Jito recommended tip API (clamped)"
                );
                clamped
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

        Ok(tip)
    }
}

#[cfg(test)]
mod tests {
    use super::{clamp_tip, derive_tip_from_balances, tip_max_lamports, tip_min_lamports};

    #[test]
    fn derives_tip_from_median_balance() {
        assert_eq!(derive_tip_from_balances(&[1_000, 2_000, 3_000]), 2_200);
    }

    #[test]
    fn tip_has_floor_when_balances_are_empty() {
        assert_eq!(derive_tip_from_balances(&[]), tip_min_lamports());
    }

    #[test]
    fn clamp_respects_bounds() {
        assert_eq!(clamp_tip(1), tip_min_lamports());
        assert_eq!(clamp_tip(999_999_999), tip_max_lamports());
    }
}

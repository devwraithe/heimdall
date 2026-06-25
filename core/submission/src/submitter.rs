use crate::{
    blockhash::{BlockhashFetcher, BlockhashMode},
    bundle::BundleConstructor,
    tip::TipCalculator,
};
use anyhow::Result;
use base64::Engine;
use jito_sdk_rust::JitoJsonRpcSDK;
use serde_json::json;
pub use shared::types::SubmissionRecord;
use tracing::{error, info, warn};

fn extract_bundle_id(response: &serde_json::Value) -> String {
    if let Some(id) = response["result"].as_str() {
        return id.to_string();
    }

    if let Some(id) = response["result"].as_array().and_then(|arr| arr.first()) {
        if let Some(id) = id.as_str() {
            return id.to_string();
        }
    }

    if let Some(id) = response["result"]["bundle_id"].as_str() {
        return id.to_string();
    }

    "unknown".to_string()
}

pub struct BundleSubmitter {
    blockhash_fetcher: BlockhashFetcher,
    tip_calculator: TipCalculator,
    bundle_constructor: BundleConstructor,
    jito_client: JitoJsonRpcSDK,
}

impl BundleSubmitter {
    pub fn new(
        rpc_url: &str,
        jito_url: &str,
        keypair: solana_sdk::signature::Keypair,
        mode: BlockhashMode,
    ) -> Self {
        Self {
            blockhash_fetcher: BlockhashFetcher::new(rpc_url, mode),
            tip_calculator: TipCalculator::new(rpc_url),
            bundle_constructor: BundleConstructor::new(keypair),
            jito_client: JitoJsonRpcSDK::new(jito_url, None),
        }
    }

    pub async fn submit_with_options(
        &self,
        slot: u64,
        leader: String,
        tip_override: Option<u64>,
        refresh_blockhash: bool,
    ) -> Result<SubmissionRecord> {
        // Fetch blockhash (real or injected)
        let blockhash = if refresh_blockhash {
            self.blockhash_fetcher.fetch_fresh()
        } else {
            self.blockhash_fetcher.fetch()
        }?;

        // Calculate tip from live data
        let tip_lamports = if let Some(override_tip) = tip_override {
            let clamped = override_tip.clamp(1_000, 100_000);
            if clamped != override_tip {
                info!(
                    override_tip,
                    clamped, "Tip override clamped to safe production bounds"
                );
            }
            clamped
        } else {
            self.tip_calculator.calculate().await?
        };

        // Build bundle transactions
        let transactions = self.bundle_constructor.build(blockhash, tip_lamports)?;
        let transaction_signatures = transactions
            .iter()
            .flat_map(|tx| tx.signatures.iter().map(|signature| signature.to_string()))
            .collect::<Vec<_>>();

        // Serialize transactions to base64
        let encoded_txns: Vec<String> = transactions
            .iter()
            .map(|tx| {
                let serialized = bincode::serialize(tx).expect("Failed to serialize transaction");
                base64::engine::general_purpose::STANDARD.encode(&serialized)
            })
            .collect();

        let params = json!([encoded_txns,  {
            "encoding": "base64",
        }]);

        // Submit bundle to Jito block engine
        let bundle_id = match self.jito_client.send_bundle(Some(params), None).await {
            Ok(response) => {
                let id = extract_bundle_id(&response);
                info!(bundle_id = %id, slot, "Bundle submitted");
                if id == "unknown" {
                    warn!(response = %response, "Jito response missing bundle id");
                }
                id
            }
            Err(e) => {
                error!(error = %e, slot, "Bundle submission failed");
                return Err(anyhow::anyhow!("Bundle submission failed: {}", e));
            }
        };

        println!("========== Submission Log: Starts ==========");
        println!("Bundle ID: {}", bundle_id);
        println!("Slot: {}", slot);
        println!("Leader: {}", leader);
        println!("Tip: {}", tip_lamports);
        println!("Blockhash: {}", blockhash);
        println!("Transaction Signatures: {:?}", transaction_signatures);
        println!("========== Submission Log: Ends ==========");

        // Produce submission record for L4
        Ok(SubmissionRecord::new(
            bundle_id,
            slot,
            leader,
            tip_lamports,
            blockhash.to_string(),
            transaction_signatures,
        ))
    }
}

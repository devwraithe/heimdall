use crate::{
    blockhash::{BlockhashFetcher, BlockhashMode},
    bundle::BundleConstructor,
    tip::TipCalculator,
    types::SubmissionRecord,
};
use anyhow::Result;
use base64::Engine;
use jito_sdk_rust::JitoJsonRpcSDK;
use serde_json::json;
use tracing::{error, info};

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

    pub async fn submit(&self, slot: u64, leader: String) -> Result<SubmissionRecord> {
        // Fetch blockhash (real or injected)
        let blockhash = self.blockhash_fetcher.fetch()?;

        // Calculate tip from live data
        let tip_lamports = self.tip_calculator.calculate()?;

        // Build bundle transactions
        let transactions = self.bundle_constructor.build(blockhash, tip_lamports)?;

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
                let id = response["result"].as_str().unwrap_or("unknown").to_string();
                info!(bundle_id = %id, slot, "Bundle submitted");
                id
            }
            Err(e) => {
                error!(error = %e, slot, "Bundle submission failed");
                return Err(anyhow::anyhow!("Bundle submission failed: {}", e));
            }
        };

        // Produce submission record for L4
        Ok(SubmissionRecord::new(
            bundle_id,
            slot,
            leader,
            tip_lamports,
            blockhash.to_string(),
        ))
    }
}

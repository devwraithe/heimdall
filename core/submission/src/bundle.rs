use anyhow::Result;
use rand::RngExt;
use solana_sdk::{
    hash::Hash, pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction,
};
use solana_system_interface::instruction as system_instruction;
use std::str::FromStr;
use tracing::info;

/// Jito's 8 tip accounts on mainnet
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

/// Assembles a Jito bundle from a payload and tip transaction
pub struct BundleConstructor {
    keypair: Keypair,
}
// Raw Jito response response={"id":1,"jsonrpc":"2.0","result":"fff128d41f52a508c2efb5c7b07b25d3b70283bff1a85d98dbb64357b147482b"}
impl BundleConstructor {
    pub fn new(keypair: Keypair) -> Self {
        Self { keypair }
    }

    /// Constructs a Jito bundle as a Vec of signed transactions.
    /// Index 0 is the payload, index 1 is the tip.
    pub fn build(&self, blockhash: Hash, tip_lamports: u64) -> Result<Vec<Transaction>> {
        let pubkey = self.keypair.pubkey();

        // Payload transaction — self transfer of 0 lamports
        // Replace with real instructions in production
        let payload_ix = system_instruction::transfer(&pubkey, &pubkey, 0);

        let payload_tx = Transaction::new_signed_with_payer(
            &[payload_ix],
            Some(&pubkey),
            &[&self.keypair],
            blockhash,
        );

        // Randomly select one of Jito's 8 tip accounts
        let tip_account_str =
            JITO_TIP_ACCOUNTS[rand::rng().random_range(0..JITO_TIP_ACCOUNTS.len())];
        let tip_account = Pubkey::from_str(tip_account_str)?;

        // Tip transaction — SOL transfer to selected tip account
        let tip_ix = system_instruction::transfer(&pubkey, &tip_account, tip_lamports);

        let tip_tx = Transaction::new_signed_with_payer(
            &[tip_ix],
            Some(&pubkey),
            &[&self.keypair],
            blockhash,
        );

        info!(
            tip_account = %tip_account_str,
            tip_lamports,
            "Bundle constructed"
        );

        Ok(vec![payload_tx, tip_tx])
    }
}

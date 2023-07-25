use anyhow::{Context, Result};
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    instruction::Instruction,
    message::{Message, VersionedMessage},
    pubkey::Pubkey,
    transaction::VersionedTransaction,
};

#[allow(clippy::module_name_repetitions)]
pub struct SolanaClient(RpcClient);

impl SolanaClient {
    #[inline]
    #[must_use]
    pub fn new(rpc: RpcClient) -> Self { Self(rpc) }

    pub async fn send_transaction(
        &self,
        instructions: &[Instruction],
        payer: Option<&Pubkey>,
        signers: &impl solana_sdk::signers::Signers,
    ) -> Result<()> {
        let rpc = &self.0;

        let txn = VersionedTransaction::try_new(
            VersionedMessage::Legacy(Message::new_with_blockhash(
                instructions,
                payer,
                &rpc.get_latest_blockhash()
                    .await
                    .context("Error getting latest blockhash")?,
            )),
            signers,
        )
        .context("Error signing transaction")?;

        let sig = rpc
            .send_transaction_with_config(&txn, RpcSendTransactionConfig {
                skip_preflight: cfg!(debug_assertions),
                ..RpcSendTransactionConfig::default()
            })
            .await
            .context("Error sending transaction")?;

        rpc.confirm_transaction_with_spinner(
            &sig,
            &rpc.get_latest_blockhash()
                .await
                .context("Error getting recent blockhash")?,
            rpc.commitment(),
        )
        .await
        .context(format!("Error confirming transaction {sig}"))?;

        println!("Success! Transaction signature: {sig}");

        Ok(())
    }
}

impl std::ops::Deref for SolanaClient {
    type Target = RpcClient;

    #[inline]
    fn deref(&self) -> &RpcClient { &self.0 }
}

//! Entrypoint for the arborist binary

#![deny(
    clippy::disallowed_methods,
    clippy::suspicious,
    clippy::style,
    clippy::clone_on_ref_ptr,
    missing_debug_implementations,
    missing_copy_implementations
)]
#![warn(clippy::pedantic, missing_docs)]

mod signer;

use std::time::Duration;

use anchor_lang::InstructionData;
use anyhow::{Context, Result};
use clap::Parser;
use solana_cli_config::Config;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    message::{Message, VersionedMessage},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
    transaction::VersionedTransaction,
};

trait ArgExt {
    fn default_solana_config(self) -> Self;
}

impl ArgExt for clap::Arg {
    fn default_solana_config(self) -> Self {
        if let Some(ref f) = *solana_cli_config::CONFIG_FILE {
            self.required(false).default_value(f.as_str())
        } else {
            self.required(true)
        }
    }
}

#[derive(Parser)]
#[command(author, version, about)]
struct Opts {
    /// Path to an existing Solana CLI configuration file
    #[arg(short = 'C', long, default_solana_config(), global = true)]
    solana_config: String,

    /// Override the default RPC endpoint
    #[arg(
        short = 'u',
        long = "url",
        value_name = "URL_OR_MONIKER",
        global = true
    )]
    rpc_url: Option<String>,

    /// Timeout for RPC requests
    #[arg(long, default_value_t = 90, global = true)]
    rpc_timeout: u64,

    /// Override the default RPC commitment level
    #[arg(long = "commitment", global = true)]
    rpc_commitment: Option<CommitmentConfig>,

    /// Override the default keypair path
    #[arg(short, long, global = true)]
    keypair: Option<String>,

    #[command(flatten)]
    signer: signer::SignerArgs,

    #[command(subcommand)]
    subcmd: Subcommand,
}

#[derive(clap::Subcommand)]
enum Subcommand {
    /// Create a new Merkle tree and tree configuration
    CreateTree(CreateTree),

    /// Delegate a Merkle tree to a new tree authority
    DelegateTree(DelegateTree),
}

#[derive(clap::Args)]
struct CreateTree {
    /// Depth (log2 capacity) of the tree
    #[arg(short, long)]
    depth: u8,

    /// Buffer size (i.e. concurrency limit) for the tree
    #[arg(short, long = "buffer")]
    buffer_size: u16,

    /// Cached tree (canopy) depth
    #[arg(short, long = "canopy", default_value_t = 0)]
    canopy_depth: u8,
}

#[derive(clap::Args)]
struct DelegateTree {
    /// Address of the Merkle tree
    #[arg(short = 't', long = "tree")]
    merkle_tree: Pubkey,

    /// Address of the tree configuration PDA
    #[arg(short = 'c', long = "config")]
    tree_authority: Pubkey,

    // TODO: this needs to be a signer
    // /// The creator of the Merkle tree, defaults to the current signer
    // #[arg(short = 'O', long = "owner")]
    // tree_owner: Option<Pubkey>,
    /// The new delegate over the Merkle tree
    #[arg(short = 'd', long = "delegate")]
    new_tree_delegate: Pubkey,
}

async fn send_transaction(
    rpc: &RpcClient,
    instructions: &[Instruction],
    payer: Option<&Pubkey>,
    signers: &impl solana_sdk::signers::Signers,
) -> Result<()> {
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

async fn create_tree(
    rpc: &RpcClient,
    keypair: &Keypair,
    pubkey: Pubkey,
    args: CreateTree,
) -> Result<()> {
    let CreateTree {
        depth,
        buffer_size,
        canopy_depth,
    } = args;

    let tree = Keypair::new();
    let tree_pubkey = tree.try_pubkey().unwrap_or_else(|_| unreachable!());

    let (tree_authority, _bump) =
        Pubkey::find_program_address(&[tree_pubkey.as_ref()], &mpl_bubblegum::ID);

    let size: u64 = {
        // TODO: if someone exports a function for doing this nicely i'm all ears

        use std::mem::size_of;

        use spl_account_compression::{
            state::CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1, ConcurrentMerkleTree,
        };

        // copied from spl-account-compression because it's mired in a labyrinth of
        // private fields and missing default impls
        fn merkle_tree_get_size(depth: u8, buffer_size: u16) -> Result<usize> {
            match (depth, buffer_size) {
                (3, 8) => Ok(size_of::<ConcurrentMerkleTree<3, 8>>()),
                (5, 8) => Ok(size_of::<ConcurrentMerkleTree<5, 8>>()),
                (14, 64) => Ok(size_of::<ConcurrentMerkleTree<14, 64>>()),
                (14, 256) => Ok(size_of::<ConcurrentMerkleTree<14, 256>>()),
                (14, 1024) => Ok(size_of::<ConcurrentMerkleTree<14, 1024>>()),
                (14, 2048) => Ok(size_of::<ConcurrentMerkleTree<14, 2048>>()),
                (15, 64) => Ok(size_of::<ConcurrentMerkleTree<15, 64>>()),
                (16, 64) => Ok(size_of::<ConcurrentMerkleTree<16, 64>>()),
                (17, 64) => Ok(size_of::<ConcurrentMerkleTree<17, 64>>()),
                (18, 64) => Ok(size_of::<ConcurrentMerkleTree<18, 64>>()),
                (19, 64) => Ok(size_of::<ConcurrentMerkleTree<19, 64>>()),
                (20, 64) => Ok(size_of::<ConcurrentMerkleTree<20, 64>>()),
                (20, 256) => Ok(size_of::<ConcurrentMerkleTree<20, 256>>()),
                (20, 1024) => Ok(size_of::<ConcurrentMerkleTree<20, 1024>>()),
                (20, 2048) => Ok(size_of::<ConcurrentMerkleTree<20, 2048>>()),
                (24, 64) => Ok(size_of::<ConcurrentMerkleTree<24, 64>>()),
                (24, 256) => Ok(size_of::<ConcurrentMerkleTree<24, 256>>()),
                (24, 512) => Ok(size_of::<ConcurrentMerkleTree<24, 512>>()),
                (24, 1024) => Ok(size_of::<ConcurrentMerkleTree<24, 1024>>()),
                (24, 2048) => Ok(size_of::<ConcurrentMerkleTree<24, 2048>>()),
                (26, 512) => Ok(size_of::<ConcurrentMerkleTree<26, 512>>()),
                (26, 1024) => Ok(size_of::<ConcurrentMerkleTree<26, 1024>>()),
                (26, 2048) => Ok(size_of::<ConcurrentMerkleTree<26, 2048>>()),
                (30, 512) => Ok(size_of::<ConcurrentMerkleTree<30, 512>>()),
                (30, 1024) => Ok(size_of::<ConcurrentMerkleTree<30, 1024>>()),
                (30, 2048) => Ok(size_of::<ConcurrentMerkleTree<30, 2048>>()),
                _ => {
                    anyhow::bail!(
                        "Invalid combination of max depth {depth} and max buffer size \
                         {buffer_size}",
                    );
                },
            }
        }
        const NODE_SIZE: u64 = 32;

        #[inline]
        const fn canopy_size(canopy_depth: u8) -> u64 { ((2 << canopy_depth) - 2) * NODE_SIZE }

        u64::try_from(
            CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1 + merkle_tree_get_size(depth, buffer_size)?,
        )
        .context("Error converting Merkle tree size to 64-bit")?
            + canopy_size(canopy_depth)
    };

    let rent = rpc
        .get_minimum_balance_for_rent_exemption(size.try_into().unwrap_or_else(|_| unreachable!()))
        .await
        .context("Error getting rent exemption balance for new tree")?;

    send_transaction(
        rpc,
        &[
            solana_sdk::system_instruction::create_account(
                &pubkey,
                &tree_pubkey,
                rent,
                size,
                &spl_account_compression::ID,
            ),
            Instruction {
                program_id: mpl_bubblegum::ID,
                accounts: vec![
                    AccountMeta::new(tree_authority, false),
                    AccountMeta::new(tree_pubkey, false),
                    AccountMeta::new_readonly(pubkey, true),
                    AccountMeta::new_readonly(pubkey, true),
                    AccountMeta::new_readonly(spl_noop::ID, false),
                    AccountMeta::new_readonly(spl_account_compression::ID, false),
                    AccountMeta::new_readonly(system_program::ID, false),
                ],
                data: mpl_bubblegum::instruction::CreateTree {
                    max_depth: depth.into(),
                    max_buffer_size: buffer_size.into(),
                    public: None, // TODO: why is this undocumented
                }
                .data(),
            },
        ],
        Some(&pubkey),
        &[keypair, &tree],
    )
    .await
}

async fn delegate_tree(
    rpc: &RpcClient,
    keypair: &Keypair,
    pubkey: Pubkey,
    args: DelegateTree,
) -> Result<()> {
    let DelegateTree {
        merkle_tree,
        tree_authority,
        // tree_owner, // TODO: for now assume the owner is always the loaded keypair
        new_tree_delegate,
    } = args;

    send_transaction(
        rpc,
        &[Instruction {
            program_id: mpl_bubblegum::ID,
            accounts: vec![
                AccountMeta::new(tree_authority, false),
                AccountMeta::new_readonly(pubkey, true),
                AccountMeta::new_readonly(new_tree_delegate, false),
                AccountMeta::new(merkle_tree, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: mpl_bubblegum::instruction::SetTreeDelegate {}.data(),
        }],
        Some(&pubkey),
        &[keypair],
    )
    .await
}

fn main() {
    match run() {
        Ok(()) => (),
        Err(e) => {
            println!("ERROR: {e:?}");
            std::process::exit(1);
        },
    }
}

fn run() -> Result<()> {
    let Opts {
        solana_config,
        rpc_url,
        rpc_timeout,
        rpc_commitment,
        keypair,
        signer,
        subcmd,
    } = Opts::parse();

    let cfg = Config::load(&solana_config)
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(Config::default())
            } else {
                Err(e)
            }
        })
        .context("Error loading Solana CLI configuration")?;

    let keypair =
        signer::keypair_from_path(&signer, &keypair.unwrap_or(cfg.keypair_path), "signer")
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("Error parsing signer keypair")?;
    let pubkey = keypair.try_pubkey().unwrap_or_else(|_| unreachable!());

    let rpc = RpcClient::new_with_timeout_and_commitment(
        solana_clap_v3_utils::input_validators::normalize_to_url_if_moniker(
            rpc_url.unwrap_or(cfg.json_rpc_url),
        ),
        Duration::from_secs(rpc_timeout),
        rpc_commitment
            .map_or_else(|| cfg.commitment.parse(), Ok)
            .context("Invalid commitment level in Solana CLI configuration")?,
    );

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Error initializing async runtime")?
        .block_on(async move {
            match subcmd {
                Subcommand::CreateTree(c) => create_tree(&rpc, &keypair, pubkey, c).await?,
                Subcommand::DelegateTree(d) => delegate_tree(&rpc, &keypair, pubkey, d).await?,
            }

            Ok(())
        })
}

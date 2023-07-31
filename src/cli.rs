use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use crate::signer;

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

#[derive(clap::Parser)]
#[command(author, version, about)]
pub struct Opts {
    /// Path to an existing Solana CLI configuration file
    #[arg(short = 'C', long, default_solana_config(), global = true)]
    pub solana_config: String,

    /// Override the default RPC endpoint
    #[arg(
        short = 'u',
        long = "url",
        value_name = "URL_OR_MONIKER",
        global = true
    )]
    pub rpc_url: Option<String>,

    /// Timeout for RPC requests
    #[arg(long, default_value_t = 90, global = true)]
    pub rpc_timeout: u64,

    /// Override the default RPC commitment level
    #[arg(long = "commitment", global = true)]
    pub rpc_commitment: Option<CommitmentConfig>,

    /// Override the default keypair path
    #[arg(short, long, global = true)]
    pub keypair: Option<String>,

    #[command(flatten)]
    pub signer: signer::SignerArgs,

    #[command(subcommand)]
    pub subcmd: Subcommand,
}

#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Create a new Merkle tree and tree configuration
    CreateTree(CreateTree),

    /// Delegate a Merkle tree to a new tree authority
    DelegateTree(DelegateTree),
}

#[derive(clap::Args)]
pub struct CreateTree {
    /// Depth (log2 capacity) of the tree
    #[arg(short, long)]
    pub depth: u8,

    /// Buffer size (i.e. concurrency limit) for the tree
    #[arg(short, long = "buffer")]
    pub buffer_size: u16,

    /// Cached tree (canopy) depth
    #[arg(short, long = "canopy", default_value_t = 0)]
    pub canopy_depth: u8,
}

#[derive(clap::Args)]
pub struct DelegateTree {
    /// Address of the Merkle tree
    #[arg(short = 't', long = "tree")]
    pub merkle_tree: Pubkey,

    /// Address of the tree configuration PDA
    #[arg(short = 'c', long = "config")]
    pub tree_authority: Pubkey,

    // TODO: this needs to be a signer
    // /// The creator of the Merkle tree, defaults to the current signer
    // #[arg(short = 'O', long = "owner")]
    // pub tree_owner: Option<Pubkey>,
    /// The new delegate over the Merkle tree
    #[arg(short = 'd', long = "delegate")]
    pub new_tree_delegate: Pubkey,
}


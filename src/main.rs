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

mod bubblegum;
mod cli;
mod signer;
mod solana;

use std::time::Duration;

use anyhow::{Context, Result};
use cli::{Opts, Subcommand};
use solana_cli_config::Config;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signer::Signer;

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
    } = clap::Parser::parse();

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

    let client = solana::SolanaClient::new(RpcClient::new_with_timeout_and_commitment(
        solana_clap_v3_utils::input_validators::normalize_to_url_if_moniker(
            rpc_url.unwrap_or(cfg.json_rpc_url),
        ),
        Duration::from_secs(rpc_timeout),
        rpc_commitment
            .map_or_else(|| cfg.commitment.parse(), Ok)
            .context("Invalid commitment level in Solana CLI configuration")?,
    ));

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Error initializing async runtime")?
        .block_on(async move {
            match subcmd {
                Subcommand::CreateTree(c) => {
                    bubblegum::create_tree(&client, &keypair, pubkey, c).await?;
                },
                Subcommand::DelegateTree(d) => {
                    bubblegum::delegate_tree(&client, &keypair, pubkey, d).await?;
                },
            }

            Ok(())
        })
}

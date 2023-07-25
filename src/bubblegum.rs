use anchor_lang::InstructionData;
use anyhow::{Context, Result};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer, system_program,
};

use crate::{cli::{CreateTree, DelegateTree}, solana::SolanaClient};

pub async fn create_tree(
    client: &SolanaClient,
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

    let rent = client
        .get_minimum_balance_for_rent_exemption(size.try_into().unwrap_or_else(|_| unreachable!()))
        .await
        .context("Error getting rent exemption balance for new tree")?;

    client.send_transaction(
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

pub async fn delegate_tree(
    client: &SolanaClient,
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

    client.send_transaction(
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

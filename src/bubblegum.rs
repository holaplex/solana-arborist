use std::{collections::BTreeMap, mem::size_of};

use anchor_lang::InstructionData;
use anyhow::{bail, Context, Result};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
};
use spl_account_compression::{state::CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1, ConcurrentMerkleTree};

use crate::{
    cli::{CreateTree, DelegateTree},
    solana::SolanaClient,
};

fn tree_size(depth: u8, buffer_size: u16, canopy_depth: u8) -> Result<u64> {
    // TODO: if someone exports a function for doing this nicely i'm all ears

    // copied from spl-account-compression because it's mired in a labyrinth of
    // private fields and missing default impls
    fn merkle_tree_get_size(depth: u8, buffer_size: u16) -> Result<usize> {
        macro_rules! tree_size {
            ($depth:expr, $buf:expr) => {
                (
                    $depth,
                    $buf,
                    size_of::<ConcurrentMerkleTree<$depth, $buf>>(),
                )
            };
        }

        lazy_static::lazy_static! {
            static ref SIZES: BTreeMap<u8, BTreeMap<u16, usize>> = {
                let mut map: BTreeMap<u8, BTreeMap<u16, usize>> = BTreeMap::new();

                for (depth, buf, size) in [
                    tree_size!(3, 8),
                    tree_size!(5, 8),
                    tree_size!(14, 64),
                    tree_size!(14, 256),
                    tree_size!(14, 1024),
                    tree_size!(14, 2048),
                    tree_size!(15, 64),
                    tree_size!(16, 64),
                    tree_size!(17, 64),
                    tree_size!(18, 64),
                    tree_size!(19, 64),
                    tree_size!(20, 64),
                    tree_size!(20, 256),
                    tree_size!(20, 1024),
                    tree_size!(20, 2048),
                    tree_size!(24, 64),
                    tree_size!(24, 256),
                    tree_size!(24, 512),
                    tree_size!(24, 1024),
                    tree_size!(24, 2048),
                    tree_size!(26, 512),
                    tree_size!(26, 1024),
                    tree_size!(26, 2048),
                    tree_size!(30, 512),
                    tree_size!(30, 1024),
                    tree_size!(30, 2048),
                ] {
                    map.entry(depth).or_default().insert(buf, size);
                }

                map
            };
        }

        let Some(map) = SIZES.get(&depth) else {
            use std::ops::Bound;

            let depths = SIZES
                .range((Bound::Unbounded, Bound::Included(depth)))
                .next()
                .into_iter()
                .chain(
                    SIZES
                        .range((Bound::Included(depth), Bound::Unbounded))
                        .next(),
                )
                .map(|(k, _)| k.to_string())
                .collect::<Vec<_>>()
                .join(" and ");

            bail!("Invalid tree depth {depth} - closest valid value(s) are {depths}")
        };

        let Some(&size) = map.get(&buffer_size) else {
            let sizes = map
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");

            bail!("Invalid buffer size {buffer_size} - valid size(s) are {sizes}");
        };

        Ok(size)
    }
    const NODE_SIZE: u64 = 32;

    #[inline]
    const fn canopy_size(canopy_depth: u8) -> u64 { ((2 << canopy_depth) - 2) * NODE_SIZE }

    Ok(u64::try_from(
        CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1 + merkle_tree_get_size(depth, buffer_size)?,
    )
    .context("Error converting Merkle tree size to 64-bit")?
        + canopy_size(canopy_depth))
}

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

    let size = tree_size(depth, buffer_size, canopy_depth)?;
    let rent = client
        .get_minimum_balance_for_rent_exemption(size.try_into().unwrap_or_else(|_| unreachable!()))
        .await
        .context("Error getting rent exemption balance for new tree")?;

    client
        .send_transaction(
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

    client
        .send_transaction(
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

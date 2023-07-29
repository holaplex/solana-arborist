# `solana-arborist`
*Keep your Merkle trees in expert hands.*

---

`solana-arborist` provides the `arborist` command-line utility, which gives
easy access to Solana programs leveraging the
[`spl-account-compression`][compression] program.  Currently the only such
supported program is the [`mpl-bubblegum`][bubblegum] compressed NFT program.

## Installation and Setup

To install Arborist, the only prerequisite you'll need is a Rust toolchain with
Cargo.

<!--
The simplest way to install Arborist is with `cargo install`:
```sh
$ cargo install solana-arborist
```
-->

To install Arborist from Git, simply check out the repo with `git clone`,
optionally select a branch or commit to install, and then install it with the
`--path` flag for `cargo install`:

```sh
$ git clone https://github.com/holaplex/solana-arborist
$ cargo install --path solana-arborist
```

Arborist loads the keypair for signing transactions in a similar fashion to the
[Solana CLI][solana-cli]. If you already have a keypair configured with the
`solana` command then it will be automatically detected by Arborist.  However,
if you use a non-standard configuration you may specify an alternate Solana
config path with `-C`, an alternate keypair location with `-k`, or the URL of
an alternate Solana RPC node with `-u`.  (Shorthands are also supported, such as
`-u devnet`)

## Usage

For additional help with the command-line interface, you can always run:

```sh
$ arborist help
```

This will print info about available commands and global options.  For help with
a specific command, run:
,
```sh
$ arborist help <COMMAND>
```

### `create-tree`

This command creates a new [concurrent Merkle tree][compression] and its
associated [bubblegum tree configuration][tree-config].  To run it, execute the
following:

```sh
$ arborist create-tree -d <DEPTH> -b <BUFFER_SIZE>
```

The `DEPTH` and `BUFFER_SIZE` parameters only accept specific value
combinations, but Arborist will attempt to both prevent submitting illegal
instruction arguments and provide help for selecting correct values for these
parameters.  For more information on the `DEPTH` and `BUFFER_SIZE` parameters,
see the [docs][tree-args].

### `delegate-tree`

This command delegates authority over an existing Merkle tree **that was
previously created with the current signing keypair** to a different public
key.  To run it, execute the following:

```sh
$ arborist delegate-tree -t <TREE> -c <TREE_AUTHORITY> -d <NEW_DELEGATE>
```

The `TREE` and `TREE_AUTHORITY` parameters are, respectively, the public keys
of the Merkle tree and Bubblegum tree configuration accounts created e.g. by the
`create-tree` subcommand.  The `NEW_DELEGATE` parameter is the public key of
the account to delegate authority of these accounts to.

[compression]: https://github.com/solana-labs/solana-program-library/tree/master/account-compression
[bubblegum]: https://github.com/metaplex-foundation/mpl-bubblegum/tree/main/programs/bubblegum
[solana-cli]: https://github.com/solana-labs/solana/tree/master/cli
[tree-config]: https://github.com/metaplex-foundation/mpl-bubblegum/tree/main/programs/bubblegum#-tree_authority
[tree-args]: https://docs.rs/spl-account-compression/0.1.3/spl_account_compression/spl_account_compression/fn.init_empty_merkle_tree.html

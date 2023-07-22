//! Copied from `solana-clap-v3-utils`.

use std::{
    convert::TryFrom,
    error,
    io::{stdin, stdout, Write},
    process::exit,
    str::FromStr,
};

use bip39::{Language, Mnemonic, Seed};
use rpassword::prompt_password;
use solana_clap_v3_utils::{
    input_parsers::STDOUT_OUTFILE_TOKEN,
    keypair::{ASK_KEYWORD, SKIP_SEED_PHRASE_VALIDATION_ARG},
};
use solana_remote_wallet::locator::{
    Locator as RemoteWalletLocator, LocatorError as RemoteWalletLocatorError,
};
use solana_sdk::{
    derivation_path::{DerivationPath, DerivationPathError},
    pubkey::Pubkey,
    signature::{
        generate_seed_from_seed_phrase_and_passphrase, keypair_from_seed,
        keypair_from_seed_and_derivation_path, keypair_from_seed_phrase_and_passphrase,
        read_keypair, read_keypair_file, Keypair, Signer,
    },
};
use thiserror::Error;

#[derive(clap::Args)]
#[allow(clippy::module_name_repetitions)]
pub struct SignerArgs {
    #[arg(
        long = SKIP_SEED_PHRASE_VALIDATION_ARG.long,
        help = SKIP_SEED_PHRASE_VALIDATION_ARG.help,
        global = true,
    )]
    skip_seed_phrase_validation: bool,

    /// Confirm key on device; only relevant if using remote wallet
    #[arg(long = "confirm-key")]
    confirm_pubkey: bool,
}

struct SignerSource {
    kind: SignerSourceKind,
    derivation_path: Option<DerivationPath>,
    legacy: bool,
}

impl SignerSource {
    fn new(kind: SignerSourceKind) -> Self {
        Self {
            kind,
            derivation_path: None,
            legacy: false,
        }
    }

    fn new_legacy(kind: SignerSourceKind) -> Self {
        Self {
            kind,
            derivation_path: None,
            legacy: true,
        }
    }
}

const SIGNER_SOURCE_PROMPT: &str = "prompt";
const SIGNER_SOURCE_FILEPATH: &str = "file";
const SIGNER_SOURCE_USB: &str = "usb";
const SIGNER_SOURCE_STDIN: &str = "stdin";
const SIGNER_SOURCE_PUBKEY: &str = "pubkey";

enum SignerSourceKind {
    Prompt,
    Filepath(String),
    Usb(RemoteWalletLocator),
    Stdin,
    Pubkey(Pubkey),
}

impl AsRef<str> for SignerSourceKind {
    fn as_ref(&self) -> &str {
        match self {
            Self::Prompt => SIGNER_SOURCE_PROMPT,
            Self::Filepath(_) => SIGNER_SOURCE_FILEPATH,
            Self::Usb(_) => SIGNER_SOURCE_USB,
            Self::Stdin => SIGNER_SOURCE_STDIN,
            Self::Pubkey(_) => SIGNER_SOURCE_PUBKEY,
        }
    }
}

impl std::fmt::Debug for SignerSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str(self.as_ref()) }
}

#[derive(Debug, Error)]
enum SignerSourceError {
    #[error("unrecognized signer source")]
    UnrecognizedSource,
    #[error(transparent)]
    RemoteWalletLocatorError(#[from] RemoteWalletLocatorError),
    #[error(transparent)]
    DerivationPathError(#[from] DerivationPathError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

fn parse_signer_source<S: AsRef<str>>(source: S) -> Result<SignerSource, SignerSourceError> {
    let source = source.as_ref();
    let source = {
        #[cfg(target_family = "windows")]
        {
            // trim matched single-quotes since cmd.exe won't
            let mut source = source;
            while let Some(trimmed) = source.strip_prefix('\'') {
                source = if let Some(trimmed) = trimmed.strip_suffix('\'') {
                    trimmed
                } else {
                    break;
                }
            }
            source.replace("\\", "/")
        }
        #[cfg(not(target_family = "windows"))]
        {
            source.to_string()
        }
    };
    match uriparse::URIReference::try_from(source.as_str()) {
        Err(_) => Err(SignerSourceError::UnrecognizedSource),
        Ok(uri) => {
            if let Some(scheme) = uri.scheme() {
                let scheme = scheme.as_str().to_ascii_lowercase();
                match scheme.as_str() {
                    SIGNER_SOURCE_PROMPT => Ok(SignerSource {
                        kind: SignerSourceKind::Prompt,
                        derivation_path: DerivationPath::from_uri_any_query(&uri)?,
                        legacy: false,
                    }),
                    SIGNER_SOURCE_FILEPATH => Ok(SignerSource::new(SignerSourceKind::Filepath(
                        uri.path().to_string(),
                    ))),
                    SIGNER_SOURCE_USB => Ok(SignerSource {
                        kind: SignerSourceKind::Usb(RemoteWalletLocator::new_from_uri(&uri)?),
                        derivation_path: DerivationPath::from_uri_key_query(&uri)?,
                        legacy: false,
                    }),
                    SIGNER_SOURCE_STDIN => Ok(SignerSource::new(SignerSourceKind::Stdin)),
                    _ => {
                        #[cfg(target_family = "windows")]
                        // On Windows, an absolute path's drive letter will be parsed as the URI
                        // scheme. Assume a filepath source in case of a single character shceme.
                        if scheme.len() == 1 {
                            return Ok(SignerSource::new(SignerSourceKind::Filepath(source)));
                        }
                        Err(SignerSourceError::UnrecognizedSource)
                    },
                }
            } else {
                match source.as_str() {
                    STDOUT_OUTFILE_TOKEN => Ok(SignerSource::new(SignerSourceKind::Stdin)),
                    ASK_KEYWORD => Ok(SignerSource::new_legacy(SignerSourceKind::Prompt)),
                    _ => match Pubkey::from_str(source.as_str()) {
                        Ok(pubkey) => Ok(SignerSource::new(SignerSourceKind::Pubkey(pubkey))),
                        Err(_) => std::fs::metadata(source.as_str())
                            .map(|_| SignerSource::new(SignerSourceKind::Filepath(source)))
                            .map_err(Into::into),
                    },
                }
            }
        },
    }
}

fn prompt_passphrase(prompt: &str) -> Result<String, Box<dyn error::Error>> {
    let passphrase = prompt_password(prompt)?;
    if !passphrase.is_empty() {
        let confirmed = rpassword::prompt_password("Enter same passphrase again: ")?;
        if confirmed != passphrase {
            return Err("Passphrases did not match".into());
        }
    }
    Ok(passphrase)
}

pub(crate) fn keypair_from_path(
    args: &SignerArgs,
    path: &str,
    keypair_name: &str,
) -> Result<Keypair, Box<dyn error::Error>> {
    let SignerSource {
        kind,
        derivation_path,
        legacy,
    } = parse_signer_source(path)?;
    match kind {
        SignerSourceKind::Prompt => Ok(keypair_from_seed_phrase(
            args,
            keypair_name,
            derivation_path,
            legacy,
        )?),
        SignerSourceKind::Filepath(path) => match read_keypair_file(&path) {
            Err(e) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "could not read keypair file \"{path}\". Run \"solana-keygen new\" to create \
                     a keypair file: {e}",
                ),
            )
            .into()),
            Ok(file) => Ok(file),
        },
        SignerSourceKind::Stdin => {
            let mut stdin = std::io::stdin();
            Ok(read_keypair(&mut stdin)?)
        },
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("signer of type `{kind:?}` does not support Keypair output",),
        )
        .into()),
    }
}

fn keypair_from_seed_phrase(
    args: &SignerArgs,
    keypair_name: &str,
    derivation_path: Option<DerivationPath>,
    legacy: bool,
) -> Result<Keypair, Box<dyn error::Error>> {
    let seed_phrase = prompt_password(format!("[{keypair_name}] seed phrase: "))?;
    let seed_phrase = seed_phrase.trim();
    let passphrase_prompt = format!(
        "[{keypair_name}] If this seed phrase has an associated passphrase, enter it now. \
         Otherwise, press ENTER to continue: ",
    );

    let keypair = if args.skip_seed_phrase_validation {
        let passphrase = prompt_passphrase(&passphrase_prompt)?;
        if legacy {
            keypair_from_seed_phrase_and_passphrase(seed_phrase, &passphrase)?
        } else {
            let seed = generate_seed_from_seed_phrase_and_passphrase(seed_phrase, &passphrase);
            keypair_from_seed_and_derivation_path(&seed, derivation_path)?
        }
    } else {
        let sanitized = sanitize_seed_phrase(seed_phrase);
        let parse_language_fn = || {
            for language in &[
                Language::English,
                Language::ChineseSimplified,
                Language::ChineseTraditional,
                Language::Japanese,
                Language::Spanish,
                Language::Korean,
                Language::French,
                Language::Italian,
            ] {
                if let Ok(mnemonic) = Mnemonic::from_phrase(&sanitized, *language) {
                    return Ok(mnemonic);
                }
            }
            Err("Can't get mnemonic from seed phrases")
        };
        let mnemonic = parse_language_fn()?;
        let passphrase = prompt_passphrase(&passphrase_prompt)?;
        let seed = Seed::new(&mnemonic, &passphrase);
        if legacy {
            keypair_from_seed(seed.as_bytes())?
        } else {
            keypair_from_seed_and_derivation_path(seed.as_bytes(), derivation_path)?
        }
    };

    if args.confirm_pubkey {
        let pubkey = keypair.pubkey();
        print!("Recovered pubkey `{pubkey:?}`. Continue? (y/n): ");
        let _ignored = stdout().flush();
        let mut input = String::new();
        stdin().read_line(&mut input).expect("Unexpected input");
        if input.to_lowercase().trim() != "y" {
            println!("Exiting");
            exit(1);
        }
    }

    Ok(keypair)
}

fn sanitize_seed_phrase(seed_phrase: &str) -> String {
    seed_phrase
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

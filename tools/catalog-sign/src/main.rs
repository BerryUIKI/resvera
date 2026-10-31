//! Offline signing tool for the Resvera production model catalog.
//!
//! The production signing seed must never be committed. Supply it through the
//! `RESVERA_CATALOG_SIGNING_KEY` environment variable as lowercase hex, or via
//! `--key-file <path>` pointing at a file containing that same hex.
//!
//! Subcommands:
//!   keygen                        Generate a fresh Ed25519 keypair (prints seed + public key)
//!   pubkey                        Print the public key derived from the supplied seed
//!   sign   --catalog <path>       Re-sign every model entry and the catalog in place
//!   verify --catalog <path>       Verify the catalog against a public key
//!
//! This tool deliberately depends on `resvera-models` and calls the same
//! `sign()` / `verify()` methods the application uses. Reimplementing the
//! canonical payload elsewhere risks a key-order or escaping difference that
//! silently produces signatures the runtime rejects.

use ed25519_dalek::{SigningKey, Verifier, VerifyingKey};
use resvera_models::catalog::{ModelCatalog, ModelCatalogEntry};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const KEY_ENV: &str = "RESVERA_CATALOG_SIGNING_KEY";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    // Support both `--flag value` and `--flag=value`.
    let args: Vec<String> = normalize_flags(args);
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        return Err("no subcommand supplied".into());
    };

    match command {
        "keygen" => keygen(),
        "pubkey" => {
            let key = load_signing_key(&args[1..])?;
            println!("{}", hex::encode(key.verifying_key().to_bytes()));
            Ok(())
        }
        "sign" => {
            let path = flag_value(&args[1..], "--catalog")?;
            let key = load_signing_key(&args[1..])?;
            sign_catalog(Path::new(&path), &key)
        }
        "verify" => {
            let path = flag_value(&args[1..], "--catalog")?;
            let public_key = load_public_key(&args[1..])?;
            verify_catalog(Path::new(&path), &public_key)
        }
        other => {
            print_usage();
            Err(format!("unknown subcommand '{other}'"))
        }
    }
}

/// Rewrites `--flag=value` into `--flag value` so `flag_value` can stay simple.
fn normalize_flags(args: &[String]) -> Vec<String> {
    let mut normalized = Vec::with_capacity(args.len());
    for arg in args {
        if let Some((flag, value)) = arg.split_once('=') {
            if flag.starts_with("--") {
                normalized.push(flag.to_string());
                normalized.push(value.to_string());
                continue;
            }
        }
        normalized.push(arg.clone());
    }
    normalized
}

fn print_usage() {
    eprintln!(
        "usage:
  resvera-catalog-sign keygen
  resvera-catalog-sign pubkey
  resvera-catalog-sign sign   --catalog <path> [--key-file <path>]
  resvera-catalog-sign verify --catalog <path> [--pubkey <hex>]

The signing seed is read from ${KEY_ENV} as lowercase hex unless --key-file is given."
    );
}

fn flag_value(args: &[String], flag: &str) -> Result<String, String> {
    let position = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| format!("missing required flag {flag}"))?;
    args.get(position + 1)
        .cloned()
        .ok_or_else(|| format!("flag {flag} requires a value"))
}

/// Reads a 32-byte seed as hex from `--key-file` or the environment.
fn read_seed_hex(args: &[String]) -> Result<String, String> {
    if let Some(position) = args.iter().position(|arg| arg == "--key-file") {
        let path = args
            .get(position + 1)
            .ok_or_else(|| "--key-file requires a value".to_string())?;
        return fs::read_to_string(path)
            .map(|content| content.trim().to_string())
            .map_err(|error| format!("cannot read key file '{path}': {error}"));
    }

    std::env::var(KEY_ENV)
        .map(|value| value.trim().to_string())
        .map_err(|_| format!("no signing seed supplied; set {KEY_ENV} or pass --key-file <path>"))
}

fn load_signing_key(args: &[String]) -> Result<SigningKey, String> {
    let hex_seed = read_seed_hex(args)?;
    let bytes =
        hex::decode(&hex_seed).map_err(|error| format!("signing seed is not valid hex: {error}"))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "signing seed must be exactly 32 bytes (64 hex characters)".to_string())?;
    Ok(SigningKey::from_bytes(&seed))
}

fn load_public_key(args: &[String]) -> Result<[u8; 32], String> {
    let hex_key = if args.iter().any(|arg| arg == "--pubkey") {
        flag_value(args, "--pubkey")?
    } else {
        // Fall back to deriving it from the supplied seed, which is convenient
        // when verifying a freshly signed catalog locally.
        hex::encode(load_signing_key(args)?.verifying_key().to_bytes())
    };
    let bytes =
        hex::decode(&hex_key).map_err(|error| format!("public key is not valid hex: {error}"))?;
    bytes
        .try_into()
        .map_err(|_| "public key must be exactly 32 bytes".to_string())
}

fn keygen() -> Result<(), String> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|error| format!("failed to gather entropy: {error}"))?;
    let key = SigningKey::from_bytes(&seed);
    // Deliberately printed to stdout for the operator to move into a secret
    // store. Never write this to a file inside the repository.
    println!("seed_hex={}", hex::encode(seed));
    println!(
        "public_key_hex={}",
        hex::encode(key.verifying_key().to_bytes())
    );
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_catalog(path: &Path) -> Result<ModelCatalog, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("cannot read catalog '{}': {error}", path.display()))?;
    serde_json::from_str(&raw).map_err(|error| format!("catalog is not valid JSON: {error}"))
}

fn write_catalog(path: &Path, catalog: &ModelCatalog) -> Result<(), String> {
    let pretty = serde_json::to_string_pretty(catalog)
        .map_err(|error| format!("cannot serialize catalog: {error}"))?;
    fs::write(path, format!("{pretty}\n"))
        .map_err(|error| format!("cannot write catalog '{}': {error}", path.display()))
}

fn sign_catalog(path: &Path, key: &SigningKey) -> Result<(), String> {
    let mut catalog = read_catalog(path)?;
    let key_bytes = key.to_bytes();

    for entry in catalog.models.iter_mut() {
        // Keep `manifest_sha256` consistent with the staged manifest bytes after
        // an edit: the downloader hashes the template string verbatim.
        refresh_manifest_digest(entry)?;
        // `load_production_catalog` verifies every entry individually, so each
        // one carries its own signature. Signing only the catalog is not enough.
        entry.sign(&key_bytes);
    }

    // The catalog signature covers the per-entry signatures, so it must be
    // produced last.
    catalog.sign(&key_bytes);

    write_catalog(path, &catalog)?;

    println!(
        "signed {} with public key {}",
        path.display(),
        hex::encode(key.verifying_key().to_bytes())
    );
    Ok(())
}

fn refresh_manifest_digest(entry: &mut ModelCatalogEntry) -> Result<(), String> {
    let Some(template) = entry.manifest_template.as_deref() else {
        return Ok(());
    };
    // Reject a malformed template now rather than at install time on a user's
    // machine.
    serde_json::from_str::<resvera_models::ModelManifest>(template)
        .map_err(|error| format!("{}: manifest_template is not a valid manifest: {error}", entry.id))?;
    entry.manifest_sha256 = sha256_hex(template.as_bytes());
    Ok(())
}

fn verify_catalog(path: &Path, public_key: &[u8; 32]) -> Result<(), String> {
    let catalog = read_catalog(path)?;
    let verifying_key = VerifyingKey::from_bytes(public_key)
        .map_err(|error| format!("invalid public key: {error}"))?;
    // `ModelCatalog::verify` is the same check the application performs.
    catalog
        .verify(public_key)
        .map_err(|error| format!("catalog signature verification failed: {error}"))?;

    for entry in &catalog.models {
        let signature_hex = entry.signature.clone();
        let signature = decode_signature(&signature_hex, &entry.id)?;
        verifying_key
            .verify(&entry.signing_payload(), &signature)
            .map_err(|_| format!("{}: signature verification failed", entry.id))?;

        if let Some(template) = entry.manifest_template.as_deref() {
            if sha256_hex(template.as_bytes()) != entry.manifest_sha256 {
                return Err(format!(
                    "{}: manifest_sha256 does not match the manifest template bytes",
                    entry.id
                ));
            }
        }
    }

    println!(
        "verified {} against public key {}",
        path.display(),
        hex::encode(public_key)
    );
    Ok(())
}

fn decode_signature(hex_signature: &str, label: &str) -> Result<ed25519_dalek::Signature, String> {
    let bytes =
        hex::decode(hex_signature).map_err(|error| format!("{label}: signature not hex: {error}"))?;
    let array: [u8; 64] = bytes
        .try_into()
        .map_err(|_| format!("{label}: signature must be 64 bytes"))?;
    Ok(ed25519_dalek::Signature::from_bytes(&array))
}

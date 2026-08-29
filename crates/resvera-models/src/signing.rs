use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, self};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid signature bytes: {0}")]
    InvalidSignatureBytes(String),
    #[error("Invalid public key bytes: {0}")]
    InvalidKeyBytes(String),
    #[error("Signature verification failed")]
    VerificationFailed,
    #[error("Hash mismatch for file {path}: expected {expected}, computed {computed}")]
    HashMismatch {
        path: String,
        expected: String,
        computed: String,
    },
}

pub fn compute_file_sha256(path: &Path) -> Result<String, io::Error> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn sign_message(signing_key: &SigningKey, message: &[u8]) -> [u8; 64] {
    let sig: Signature = signing_key.sign(message);
    sig.to_bytes()
}

pub fn verify_signature(
    verifying_key_bytes: &[u8; 32],
    message: &[u8],
    signature_bytes: &[u8; 64],
) -> Result<(), SigningError> {
    let vk = VerifyingKey::from_bytes(verifying_key_bytes)
        .map_err(|e| SigningError::InvalidKeyBytes(e.to_string()))?;
    let sig = Signature::from_bytes(signature_bytes);
    vk.verify(message, &sig)
        .map_err(|_| SigningError::VerificationFailed)
}

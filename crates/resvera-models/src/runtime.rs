use crate::signing::SigningError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Signing error: {0}")]
    Signing(#[from] SigningError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("Component not found: {0}")]
    NotFound(String),
    #[error("Installation failed: {0}")]
    InstallFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeArtifact {
    pub name: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeComponentManifest {
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub target_platform: String,
    pub min_engine_version: String,
    pub artifacts: Vec<RuntimeArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCatalog {
    pub schema_version: u32,
    pub updated_at: String,
    pub components: Vec<RuntimeComponentManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedRuntimeCatalog {
    pub payload: String,
    pub signature: String,
}

impl SignedRuntimeCatalog {
    pub fn verify_and_parse(&self, public_key_hex: &str) -> Result<RuntimeCatalog, RuntimeError> {
        let pk_vec = hex::decode(public_key_hex)
            .map_err(|e| SigningError::InvalidKeyBytes(e.to_string()))?;
        if pk_vec.len() != 32 {
            return Err(SigningError::InvalidKeyBytes("Public key must be 32 bytes".into()).into());
        }
        let mut pk_arr = [0u8; 32];
        pk_arr.copy_from_slice(&pk_vec);

        crate::signing::verify_signature_hex(
            &pk_arr,
            self.payload.as_bytes(),
            &self.signature,
        )?;
        let catalog: RuntimeCatalog = serde_json::from_str(&self.payload)?;
        Ok(catalog)
    }
}

pub struct RuntimeInstaller {
    pub runtime_dir: PathBuf,
}

impl RuntimeInstaller {
    pub fn new<P: AsRef<Path>>(runtime_dir: P) -> Self {
        Self {
            runtime_dir: runtime_dir.as_ref().to_path_buf(),
        }
    }

    pub fn install_component(
        &self,
        manifest: &RuntimeComponentManifest,
        staged_files: &[(PathBuf, String)], // (source_path, expected_sha256)
    ) -> Result<PathBuf, RuntimeError> {
        // 1. Verify all staged files match expected SHA-256
        for (file_path, expected_hash) in staged_files {
            let data = fs::read(file_path)?;
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let actual_hash = hex::encode(hasher.finalize());
            if !actual_hash.eq_ignore_ascii_case(expected_hash) {
                return Err(RuntimeError::HashMismatch {
                    expected: expected_hash.clone(),
                    actual: actual_hash,
                });
            }
        }

        let target_dir = self.runtime_dir.join(&manifest.id).join(&manifest.version);
        let backup_dir = self.runtime_dir.join(&manifest.id).join(format!("{}.backup", manifest.version));

        if target_dir.exists() {
            let _ = fs::rename(&target_dir, &backup_dir);
        }

        fs::create_dir_all(&target_dir)?;

        for (file_path, _) in staged_files {
            let file_name = file_path.file_name().unwrap();
            let dest_path = target_dir.join(file_name);
            if let Err(e) = fs::copy(file_path, &dest_path) {
                // Rollback
                let _ = fs::remove_dir_all(&target_dir);
                if backup_dir.exists() {
                    let _ = fs::rename(&backup_dir, &target_dir);
                }
                return Err(RuntimeError::InstallFailed(e.to_string()));
            }
        }

        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }

        Ok(target_dir)
    }

    pub fn rollback_component(&self, component_id: &str, target_version: &str) -> Result<(), RuntimeError> {
        let comp_dir = self.runtime_dir.join(component_id);
        let version_dir = comp_dir.join(target_version);
        if !version_dir.exists() {
            return Err(RuntimeError::NotFound(format!(
                "Version {} of {} not available for rollback",
                target_version, component_id
            )));
        }
        // Active link/version is established
        let active_marker = comp_dir.join("active_version.txt");
        fs::write(active_marker, target_version)?;
        Ok(())
    }
}

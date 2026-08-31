use crate::catalog::ModelCatalogEntry;
use crate::installer::{InstallerError, ModelInstaller};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Install error: {0}")]
    Install(#[from] InstallerError),
    #[error("Hash mismatch: expected {expected}, calculated {calculated}")]
    HashMismatch {
        expected: String,
        calculated: String,
    },
    #[error("Signature invalid: {0}")]
    SignatureInvalid(String),
    #[error("Download cancelled")]
    Cancelled,
}

pub struct StagedDownloader {
    base_dir: PathBuf,
}

impl StagedDownloader {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Simulates receiving staged chunks, verifying SHA-256, and installing.
    pub fn stage_and_install(
        &self,
        entry: &ModelCatalogEntry,
        data_chunks: &[&[u8]],
        manifest_json: &str,
        public_key: &[u8; 32],
    ) -> Result<PathBuf, DownloadError> {
        crate::validate_path_component(&entry.id, "entry.id")
            .map_err(DownloadError::SignatureInvalid)?;
        crate::validate_path_component(&entry.version, "entry.version")
            .map_err(DownloadError::SignatureInvalid)?;
        entry
            .verify(public_key)
            .map_err(|error| DownloadError::SignatureInvalid(error.to_string()))?;

        let manifest_hash = format!("{:x}", Sha256::digest(manifest_json.as_bytes()));
        if !manifest_hash.eq_ignore_ascii_case(&entry.manifest_sha256) {
            return Err(DownloadError::SignatureInvalid(
                "Package manifest digest does not match the signed catalog entry".into(),
            ));
        }
        let manifest: crate::ModelManifest = serde_json::from_str(manifest_json)
            .map_err(|error| DownloadError::SignatureInvalid(error.to_string()))?;
        manifest
            .validate()
            .map_err(|error| DownloadError::SignatureInvalid(error.to_string()))?;
        if manifest.id != entry.id || manifest.package_version != entry.version {
            return Err(DownloadError::SignatureInvalid(
                "Manifest identity does not match the signed catalog entry".into(),
            ));
        }
        let matching_artifact = manifest.artifacts.iter().find(|artifact| {
            artifact.path == "artifacts/model.onnx"
                && artifact.size_bytes == entry.size_bytes
                && artifact.sha256.eq_ignore_ascii_case(&entry.sha256)
        });
        if manifest.artifacts.len() != 1 || matching_artifact.is_none() {
            return Err(DownloadError::SignatureInvalid(
                "Manifest artifact contract does not match the signed catalog entry".into(),
            ));
        }

        let staged_root = self.base_dir.join(".staged");
        let stage_dir = staged_root.join(&entry.id).join(&entry.version);
        let artifacts_dir = stage_dir.join("artifacts");
        fs::create_dir_all(&artifacts_dir)?;

        let artifact_path = artifacts_dir.join("model.onnx");
        let mut hasher = Sha256::new();
        let mut file = File::create(&artifact_path)?;

        for chunk in data_chunks {
            file.write_all(chunk)?;
            hasher.update(chunk);
        }
        file.flush()?;
        drop(file);

        let calculated_hash = format!("{:x}", hasher.finalize());
        let calculated_size = fs::metadata(&artifact_path)?.len();
        if calculated_size != entry.size_bytes
            || !calculated_hash.eq_ignore_ascii_case(&entry.sha256)
        {
            self.cleanup_stage_dir(&entry.id, &entry.version);
            return Err(DownloadError::HashMismatch {
                expected: format!("{} ({} bytes)", entry.sha256, entry.size_bytes),
                calculated: format!("{} ({} bytes)", calculated_hash, calculated_size),
            });
        }

        // Write package manifest.json
        let manifest_path = stage_dir.join("manifest.json");
        fs::write(&manifest_path, manifest_json)?;

        // Now install from staged directory
        let installer = ModelInstaller::new(&self.base_dir);
        let manifest = match installer.install_package(&stage_dir) {
            Ok(m) => m,
            Err(e) => {
                self.cleanup_stage_dir(&entry.id, &entry.version);
                return Err(DownloadError::Install(e));
            }
        };

        self.cleanup_stage_dir(&entry.id, &entry.version);

        let installed_dir = self
            .base_dir
            .join(&manifest.id)
            .join(&manifest.package_version);
        Ok(installed_dir)
    }

    fn cleanup_stage_dir(&self, entry_id: &str, entry_version: &str) {
        let staged_root = self.base_dir.join(".staged");
        let model_dir = staged_root.join(entry_id);
        let stage_dir = model_dir.join(entry_version);
        let _ = fs::remove_dir_all(&stage_dir);
        let _ = fs::remove_dir(&model_dir);
        let _ = fs::remove_dir(&staged_root);
    }
}

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
    ) -> Result<PathBuf, DownloadError> {
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
        if calculated_hash != entry.sha256 {
            let _ = fs::remove_dir_all(&staged_root);
            return Err(DownloadError::HashMismatch {
                expected: entry.sha256.clone(),
                calculated: calculated_hash,
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
                let _ = fs::remove_dir_all(&staged_root);
                return Err(DownloadError::Install(e));
            }
        };

        let _ = fs::remove_dir_all(&staged_root);

        let installed_dir = self.base_dir.join(&manifest.id).join(&manifest.package_version);
        Ok(installed_dir)
    }
}

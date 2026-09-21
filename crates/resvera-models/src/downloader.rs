use crate::catalog::ModelCatalogEntry;
use crate::installer::{InstallerError, ModelInstaller};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
        let mut cursor = io::Cursor::new(data_chunks.concat());
        self.stage_and_install_reader(entry, &mut cursor, manifest_json, public_key, None, None)
    }

    /// Streams an artifact from any `Read` source with SHA-256 validation,
    /// cooperative cancellation, progress reporting, and transactional installation.
    pub fn stage_and_install_reader<R: Read>(
        &self,
        entry: &ModelCatalogEntry,
        reader: &mut R,
        manifest_json: &str,
        public_key: &[u8; 32],
        cancel_token: Option<&AtomicBool>,
        mut progress_cb: Option<&mut dyn FnMut(u64, u64)>,
    ) -> Result<PathBuf, DownloadError> {
        crate::validate_path_component(&entry.id, "entry.id")
            .map_err(DownloadError::SignatureInvalid)?;
        crate::validate_path_component(&entry.version, "entry.version")
            .map_err(DownloadError::SignatureInvalid)?;

        entry
            .verify(public_key)
            .map_err(|error| DownloadError::SignatureInvalid(error.to_string()))?;

        let manifest_content = if manifest_json.is_empty() {
            entry.manifest_template.as_deref().ok_or_else(|| {
                DownloadError::SignatureInvalid("Manifest template missing in catalog entry".into())
            })?
        } else {
            manifest_json
        };

        let manifest_hash = format!("{:x}", Sha256::digest(manifest_content.as_bytes()));
        if !manifest_hash.eq_ignore_ascii_case(&entry.manifest_sha256) {
            return Err(DownloadError::SignatureInvalid(
                "Package manifest digest does not match the signed catalog entry".into(),
            ));
        }

        let manifest: crate::ModelManifest = serde_json::from_str(manifest_content)
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

        let mut buffer = [0u8; 64 * 1024];
        let mut total_downloaded: u64 = 0;

        loop {
            if let Some(token) = cancel_token {
                if token.load(Ordering::Relaxed) {
                    drop(file);
                    self.cleanup_stage_dir(&entry.id, &entry.version);
                    return Err(DownloadError::Cancelled);
                }
            }

            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            file.write_all(&buffer[..bytes_read])?;
            hasher.update(&buffer[..bytes_read]);
            total_downloaded += bytes_read as u64;

            if let Some(ref mut cb) = progress_cb {
                cb(total_downloaded, entry.size_bytes);
            }
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
        fs::write(&manifest_path, manifest_content)?;

        // Now install from staged directory
        let installer = ModelInstaller::new(&self.base_dir);
        let installed_manifest = match installer.install_package(&stage_dir) {
            Ok(m) => m,
            Err(e) => {
                self.cleanup_stage_dir(&entry.id, &entry.version);
                return Err(DownloadError::Install(e));
            }
        };

        self.cleanup_stage_dir(&entry.id, &entry.version);

        let installed_dir = self
            .base_dir
            .join(&installed_manifest.id)
            .join(&installed_manifest.package_version);
        Ok(installed_dir)
    }

    /// Imports a model from a local file, validating it against the signed catalog entry.
    pub fn import_local_file<P: AsRef<Path>>(
        &self,
        entry: &ModelCatalogEntry,
        source_path: P,
        manifest_json: &str,
        public_key: &[u8; 32],
    ) -> Result<PathBuf, DownloadError> {
        let mut file = File::open(source_path.as_ref())?;
        self.stage_and_install_reader(entry, &mut file, manifest_json, public_key, None, None)
    }

    /// Sweeps stale staging directories left behind by crashes or interrupted downloads.
    pub fn sweep_stale_staging_dirs(&self) -> io::Result<usize> {
        let mut cleaned = 0;
        let staged_root = self.base_dir.join(".staged");
        if staged_root.exists() {
            let _ = fs::remove_dir_all(&staged_root);
            cleaned += 1;
        }

        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if path.is_dir()
                    && (name_str.starts_with(".staging-") || name_str.starts_with(".backup-"))
                {
                    let _ = fs::remove_dir_all(&path);
                    cleaned += 1;
                } else if path.is_dir() {
                    if let Ok(sub_entries) = fs::read_dir(&path) {
                        for sub in sub_entries.flatten() {
                            let sub_path = sub.path();
                            let sub_name = sub.file_name();
                            let sub_name_str = sub_name.to_string_lossy();
                            if sub_path.is_dir() && sub_name_str.starts_with(".backup-") {
                                let _ = fs::remove_dir_all(&sub_path);
                                cleaned += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(cleaned)
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

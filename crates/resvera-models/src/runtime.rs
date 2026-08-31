use crate::signing::SigningError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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

        crate::signing::verify_signature_hex(&pk_arr, self.payload.as_bytes(), &self.signature)?;
        let catalog: RuntimeCatalog = serde_json::from_str(&self.payload)?;
        catalog.validate()?;
        Ok(catalog)
    }
}

impl RuntimeCatalog {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.schema_version != 1 {
            return Err(RuntimeError::InstallFailed(format!(
                "Unsupported runtime catalog schema {}",
                self.schema_version
            )));
        }
        let mut component_keys = HashSet::new();
        for component in &self.components {
            crate::validate_path_component(&component.id, "component.id")
                .map_err(RuntimeError::InstallFailed)?;
            crate::validate_path_component(&component.version, "component.version")
                .map_err(RuntimeError::InstallFailed)?;
            if !component_keys.insert((
                &component.id,
                &component.version,
                &component.target_platform,
            )) {
                return Err(RuntimeError::InstallFailed(format!(
                    "Duplicate runtime component {} {} for {}",
                    component.id, component.version, component.target_platform
                )));
            }
            if component.artifacts.is_empty() {
                return Err(RuntimeError::InstallFailed(format!(
                    "Runtime component {} has no artifacts",
                    component.id
                )));
            }
            let mut names = HashSet::new();
            for artifact in &component.artifacts {
                crate::validate_path_component(&artifact.name, "artifact.name")
                    .map_err(RuntimeError::InstallFailed)?;
                if !names.insert(&artifact.name) {
                    return Err(RuntimeError::InstallFailed(format!(
                        "Duplicate runtime artifact {}",
                        artifact.name
                    )));
                }
                if artifact.sha256.len() != 64
                    || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(RuntimeError::InstallFailed(format!(
                        "Runtime artifact '{}' has an invalid SHA-256 digest",
                        artifact.name
                    )));
                }
            }
        }
        Ok(())
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
        staged_files: &[PathBuf],
    ) -> Result<PathBuf, RuntimeError> {
        crate::validate_path_component(&manifest.id, "manifest.id")
            .map_err(RuntimeError::InstallFailed)?;
        crate::validate_path_component(&manifest.version, "manifest.version")
            .map_err(RuntimeError::InstallFailed)?;
        RuntimeCatalog {
            schema_version: 1,
            updated_at: String::new(),
            components: vec![manifest.clone()],
        }
        .validate()?;

        if staged_files.len() != manifest.artifacts.len() {
            return Err(RuntimeError::InstallFailed(format!(
                "Expected {} staged artifacts, got {}",
                manifest.artifacts.len(),
                staged_files.len()
            )));
        }
        let declared: HashMap<&str, &RuntimeArtifact> = manifest
            .artifacts
            .iter()
            .map(|artifact| (artifact.name.as_str(), artifact))
            .collect();
        let mut staged_by_name = HashMap::new();
        for path in staged_files {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| RuntimeError::InstallFailed("Invalid staged file name".into()))?;
            if !path.is_file()
                || !declared.contains_key(name)
                || staged_by_name.insert(name, path).is_some()
            {
                return Err(RuntimeError::InstallFailed(format!(
                    "Staged artifact '{name}' is missing, duplicate, or undeclared"
                )));
            }
        }

        // Validate every staged artifact before moving an already-installed version aside.
        for artifact in &manifest.artifacts {
            verify_runtime_artifact(staged_by_name[artifact.name.as_str()], artifact)?;
        }

        let component_dir = self.runtime_dir.join(&manifest.id);
        let target_dir = component_dir.join(&manifest.version);
        let backup_dir = component_dir.join(format!(
            "{}.backup.{}",
            manifest.version,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&component_dir)?;

        if target_dir.exists() {
            fs::rename(&target_dir, &backup_dir)?;
        }

        if let Err(error) = fs::create_dir(&target_dir) {
            restore_runtime_backup(&backup_dir, &target_dir);
            return Err(RuntimeError::Io(error));
        }

        for artifact in &manifest.artifacts {
            let source_path = staged_by_name[artifact.name.as_str()];
            let dest_path = target_dir.join(&artifact.name);
            if let Err(error) = fs::copy(source_path, &dest_path) {
                let _ = fs::remove_dir_all(&target_dir);
                restore_runtime_backup(&backup_dir, &target_dir);
                return Err(RuntimeError::InstallFailed(error.to_string()));
            }
        }

        for artifact in &manifest.artifacts {
            let dest_path = target_dir.join(&artifact.name);
            if let Err(error) = verify_runtime_artifact(&dest_path, artifact) {
                let _ = fs::remove_dir_all(&target_dir);
                restore_runtime_backup(&backup_dir, &target_dir);
                return Err(error);
            }
        }

        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }

        Ok(target_dir)
    }

    pub fn rollback_component(
        &self,
        component_id: &str,
        target_version: &str,
    ) -> Result<(), RuntimeError> {
        crate::validate_path_component(component_id, "component_id")
            .map_err(RuntimeError::InstallFailed)?;
        crate::validate_path_component(target_version, "target_version")
            .map_err(RuntimeError::InstallFailed)?;
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
        let temp_marker = comp_dir.join(format!("active_version.tmp.{}", uuid::Uuid::new_v4()));
        let backup_marker =
            comp_dir.join(format!("active_version.backup.{}", uuid::Uuid::new_v4()));
        fs::write(&temp_marker, target_version)?;
        if active_marker.exists() {
            if let Err(error) = fs::rename(&active_marker, &backup_marker) {
                let _ = fs::remove_file(&temp_marker);
                return Err(RuntimeError::Io(error));
            }
        }
        if let Err(error) = fs::rename(&temp_marker, active_marker) {
            let _ = fs::remove_file(temp_marker);
            if backup_marker.exists() {
                let _ = fs::rename(backup_marker, comp_dir.join("active_version.txt"));
            }
            return Err(RuntimeError::Io(error));
        }
        if backup_marker.exists() {
            let _ = fs::remove_file(backup_marker);
        }
        Ok(())
    }
}

fn verify_runtime_artifact(
    artifact_path: &Path,
    artifact: &RuntimeArtifact,
) -> Result<(), RuntimeError> {
    let actual_size = fs::metadata(artifact_path)?.len();
    let actual_hash = crate::signing::compute_file_sha256(artifact_path)?;
    if actual_size != artifact.size_bytes || !actual_hash.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(RuntimeError::HashMismatch {
            expected: format!("{} ({} bytes)", artifact.sha256, artifact.size_bytes),
            actual: format!("{} ({} bytes)", actual_hash, actual_size),
        });
    }
    Ok(())
}

fn restore_runtime_backup(backup_dir: &Path, target_dir: &Path) {
    if backup_dir.exists() && !target_dir.exists() {
        let _ = fs::rename(backup_dir, target_dir);
    }
}

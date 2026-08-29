use crate::manifest::{ManifestError, ModelManifest};
use crate::signing::{compute_file_sha256, SigningError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InstallerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("Signing error: {0}")]
    Signing(#[from] SigningError),
    #[error("Corrupted package: {0}")]
    CorruptPackage(String),
    #[error("Package version not found: {0}")]
    VersionNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentPointer {
    pub model_id: String,
    pub active_version: String,
    pub activated_at: String,
}

pub struct ModelInstaller {
    pub models_root: PathBuf,
}

impl ModelInstaller {
    pub fn new<P: Into<PathBuf>>(models_root: P) -> Self {
        Self {
            models_root: models_root.into(),
        }
    }

    /// Verifies all artifacts and files in a staged package directory against its manifest.
    pub fn verify_package_dir(&self, staged_dir: &Path) -> Result<ModelManifest, InstallerError> {
        let manifest_path = staged_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(InstallerError::CorruptPackage("Missing manifest.json".into()));
        }

        let manifest = ModelManifest::load_from_file(&manifest_path)?;

        // Verify SHA-256 for all declared artifacts
        for artifact in &manifest.artifacts {
            let file_path = staged_dir.join(&artifact.path);
            if !file_path.exists() {
                return Err(InstallerError::CorruptPackage(format!(
                    "Missing artifact file: {}",
                    artifact.path
                )));
            }

            let computed_hash = compute_file_sha256(&file_path)?;
            if computed_hash.to_lowercase() != artifact.sha256.to_lowercase() {
                return Err(InstallerError::Signing(SigningError::HashMismatch {
                    path: artifact.path.clone(),
                    expected: artifact.sha256.clone(),
                    computed: computed_hash,
                }));
            }
        }

        Ok(manifest)
    }

    /// Atomically installs a verified package from a transaction staging directory into the permanent registry.
    pub fn install_package(
        &self,
        staged_dir: &Path,
    ) -> Result<ModelManifest, InstallerError> {
        let manifest = self.verify_package_dir(staged_dir)?;

        let model_dir = self.models_root.join(&manifest.id);
        let version_dir = model_dir.join(&manifest.package_version);

        fs::create_dir_all(&model_dir)?;

        if version_dir.exists() {
            // Remove existing version directory if replacing
            fs::remove_dir_all(&version_dir)?;
        }

        // Atomically rename/move staged directory to permanent version directory
        fs::rename(staged_dir, &version_dir)?;

        // Atomically activate this version
        self.activate_version(&manifest.id, &manifest.package_version)?;

        Ok(manifest)
    }

    /// Activates a specific installed package version by updating current.json atomically.
    pub fn activate_version(
        &self,
        model_id: &str,
        package_version: &str,
    ) -> Result<(), InstallerError> {
        let model_dir = self.models_root.join(model_id);
        let version_dir = model_dir.join(package_version);

        if !version_dir.exists() {
            return Err(InstallerError::VersionNotFound(format!(
                "Version {} of model {} is not installed",
                package_version, model_id
            )));
        }

        let pointer = CurrentPointer {
            model_id: model_id.to_string(),
            active_version: package_version.to_string(),
            activated_at: chrono::Utc::now().to_rfc3339(),
        };

        let temp_file = model_dir.join("current.json.tmp");
        let target_file = model_dir.join("current.json");

        let json_data = serde_json::to_string_pretty(&pointer)
            .map_err(|e| InstallerError::Manifest(ManifestError::Json(e)))?;
        fs::write(&temp_file, json_data)?;
        fs::rename(&temp_file, &target_file)?;

        Ok(())
    }

    /// Reads the currently active version for a model.
    pub fn get_active_version(&self, model_id: &str) -> Result<Option<String>, InstallerError> {
        let current_file = self.models_root.join(model_id).join("current.json");
        if !current_file.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(current_file)?;
        let pointer: CurrentPointer = serde_json::from_str(&data)
            .map_err(|e| InstallerError::Manifest(ManifestError::Json(e)))?;
        Ok(Some(pointer.active_version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;
    use tempfile::tempdir;

    fn make_test_manifest(id: &str, version: &str, artifact_file: &str, hash: &str) -> ModelManifest {
        ModelManifest {
            schema_version: 1,
            id: id.into(),
            package_version: version.into(),
            display_name: "Test Model".into(),
            family: "rrdb".into(),
            category: "photo".into(),
            description: "Test".into(),
            license: LicenseSpec {
                spdx: "BSD-3-Clause".into(),
                upstream_url: "https://example.com".into(),
                redistribution_review: "approved".into(),
            },
            provenance: ProvenanceSpec {
                upstream_repository: "https://example.com".into(),
                upstream_revision: "abcdef".into(),
                source_weight_name: "test.pth".into(),
                source_weight_sha256: "1234".into(),
                export_recipe: "recipe.toml".into(),
            },
            variants: vec![ModelVariant {
                id: "default".into(),
                native_scale: 4,
                strength: None,
                artifact: format!("artifacts/{}", artifact_file),
            }],
            tensor: TensorSpec {
                input_name: "input".into(),
                output_name: "output".into(),
                layout: "NCHW".into(),
                channels: "RGB".into(),
                input_range: [0.0, 1.0],
                output_range: [0.0, 1.0],
                element_type: "float32".into(),
            },
            tiling: TilingSpec {
                alignment: 1,
                minimum: 32,
                recommended: 256,
                overlap: 16,
                window_size: None,
                static_shapes_required: false,
            },
            compatibility: CompatibilitySpec {
                engine: "onnx-runtime".into(),
                minimum_engine_version: "1.16".into(),
                validated_providers: vec!["cpu".into()],
                validated_precisions: vec!["fp32".into()],
            },
            artifacts: vec![ArtifactEntry {
                path: format!("artifacts/{}", artifact_file),
                size_bytes: 4,
                sha256: hash.into(),
            }],
        }
    }

    #[test]
    fn test_install_verify_and_rollback() {
        let root = tempdir().unwrap();
        let installer = ModelInstaller::new(root.path());

        // 1. Stage Version 1.0.0
        let stage_v1 = tempdir().unwrap();
        let art_dir_v1 = stage_v1.path().join("artifacts");
        fs::create_dir_all(&art_dir_v1).unwrap();
        let art_file_v1 = art_dir_v1.join("model.onnx");
        fs::write(&art_file_v1, b"v1.0 model bytes").unwrap();
        let hash_v1 = compute_file_sha256(&art_file_v1).unwrap();

        let manifest_v1 = make_test_manifest("realesrgan-x4plus", "1.0.0", "model.onnx", &hash_v1);
        fs::write(
            stage_v1.path().join("manifest.json"),
            serde_json::to_string_pretty(&manifest_v1).unwrap(),
        )
        .unwrap();

        // Install v1.0.0
        installer.install_package(stage_v1.path()).unwrap();
        assert_eq!(
            installer.get_active_version("realesrgan-x4plus").unwrap(),
            Some("1.0.0".into())
        );

        // 2. Stage Version 2.0.0
        let stage_v2 = tempdir().unwrap();
        let art_dir_v2 = stage_v2.path().join("artifacts");
        fs::create_dir_all(&art_dir_v2).unwrap();
        let art_file_v2 = art_dir_v2.join("model.onnx");
        fs::write(&art_file_v2, b"v2.0 updated bytes").unwrap();
        let hash_v2 = compute_file_sha256(&art_file_v2).unwrap();

        let manifest_v2 = make_test_manifest("realesrgan-x4plus", "2.0.0", "model.onnx", &hash_v2);
        fs::write(
            stage_v2.path().join("manifest.json"),
            serde_json::to_string_pretty(&manifest_v2).unwrap(),
        )
        .unwrap();

        // Install v2.0.0
        installer.install_package(stage_v2.path()).unwrap();
        assert_eq!(
            installer.get_active_version("realesrgan-x4plus").unwrap(),
            Some("2.0.0".into())
        );

        // 3. Rollback to Version 1.0.0
        installer.activate_version("realesrgan-x4plus", "1.0.0").unwrap();
        assert_eq!(
            installer.get_active_version("realesrgan-x4plus").unwrap(),
            Some("1.0.0".into())
        );

        // 4. Test Corrupted Artifact Detection
        let stage_corrupt = tempdir().unwrap();
        let art_dir_c = stage_corrupt.path().join("artifacts");
        fs::create_dir_all(&art_dir_c).unwrap();
        let art_file_c = art_dir_c.join("model.onnx");
        fs::write(&art_file_c, b"tampered bytes").unwrap();

        // Manifest declares hash_v1 but file contains tampered bytes
        let manifest_corrupt = make_test_manifest("realesrgan-x4plus", "3.0.0", "model.onnx", &hash_v1);
        fs::write(
            stage_corrupt.path().join("manifest.json"),
            serde_json::to_string_pretty(&manifest_corrupt).unwrap(),
        )
        .unwrap();

        let result = installer.install_package(stage_corrupt.path());
        assert!(result.is_err());
        // Active version should still be 1.0.0 untouched!
        assert_eq!(
            installer.get_active_version("realesrgan-x4plus").unwrap(),
            Some("1.0.0".into())
        );
    }
}

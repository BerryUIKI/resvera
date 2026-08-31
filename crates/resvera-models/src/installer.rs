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

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub manifest: ModelManifest,
    pub variant: crate::manifest::ModelVariant,
    pub artifact_path: PathBuf,
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
        let package_root = staged_dir.canonicalize()?;
        if !package_root.is_dir() {
            return Err(InstallerError::CorruptPackage(
                "Package root is not a directory".into(),
            ));
        }
        let manifest_path = staged_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(InstallerError::CorruptPackage(
                "Missing manifest.json".into(),
            ));
        }

        let manifest = ModelManifest::load_from_file(&manifest_path)?;

        // Verify SHA-256 for all declared artifacts
        for artifact in &manifest.artifacts {
            let file_path = staged_dir.join(&artifact.path);
            let canonical_path = file_path.canonicalize().map_err(|_| {
                InstallerError::CorruptPackage(format!("Missing artifact file: {}", artifact.path))
            })?;
            if !canonical_path.starts_with(&package_root) || !canonical_path.is_file() {
                return Err(InstallerError::CorruptPackage(format!(
                    "Artifact escapes the package root or is not a regular file: {}",
                    artifact.path
                )));
            }

            let actual_size = fs::metadata(&canonical_path)?.len();
            if actual_size != artifact.size_bytes {
                return Err(InstallerError::CorruptPackage(format!(
                    "Artifact '{}' size mismatch: expected {}, got {}",
                    artifact.path, artifact.size_bytes, actual_size
                )));
            }

            let computed_hash = compute_file_sha256(&canonical_path)?;
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
    pub fn install_package(&self, staged_dir: &Path) -> Result<ModelManifest, InstallerError> {
        // Verify before touching an installed version. Destination verification below protects
        // against changes between validation and activation.
        let manifest = self.verify_package_dir(staged_dir)?;

        let model_dir = self.models_root.join(&manifest.id);
        let version_dir = model_dir.join(&manifest.package_version);
        let backup_dir = model_dir.join(format!(
            ".backup-{}-{}",
            manifest.package_version,
            uuid::Uuid::new_v4()
        ));

        fs::create_dir_all(&model_dir)?;

        if version_dir.exists() {
            fs::rename(&version_dir, &backup_dir)?;
        }

        if let Err(error) = fs::rename(staged_dir, &version_dir) {
            restore_backup(&backup_dir, &version_dir);
            return Err(InstallerError::Io(error));
        }

        let verified_manifest = match self.verify_package_dir(&version_dir) {
            Ok(m) => m,
            Err(e) => {
                let _ = fs::remove_dir_all(&version_dir);
                restore_backup(&backup_dir, &version_dir);
                return Err(e);
            }
        };

        if let Err(e) =
            self.activate_version(&verified_manifest.id, &verified_manifest.package_version)
        {
            let _ = fs::remove_dir_all(&version_dir);
            restore_backup(&backup_dir, &version_dir);
            return Err(e);
        }

        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }

        Ok(verified_manifest)
    }

    /// Activates a specific installed package version by updating current.json atomically.
    pub fn activate_version(
        &self,
        model_id: &str,
        package_version: &str,
    ) -> Result<(), InstallerError> {
        crate::validate_path_component(model_id, "model_id")
            .map_err(InstallerError::CorruptPackage)?;
        crate::validate_path_component(package_version, "package_version")
            .map_err(InstallerError::CorruptPackage)?;
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

        let temp_file = model_dir.join(format!("current.json.tmp.{}", uuid::Uuid::new_v4()));
        let target_file = model_dir.join("current.json");

        let json_data = serde_json::to_string_pretty(&pointer)
            .map_err(|e| InstallerError::Manifest(ManifestError::Json(e)))?;
        fs::write(&temp_file, json_data)?;
        if let Err(e) = fs::rename(&temp_file, &target_file) {
            let _ = fs::remove_file(&temp_file);
            return Err(InstallerError::Io(e));
        }

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
        if pointer.model_id != model_id {
            return Err(InstallerError::CorruptPackage(format!(
                "Activation pointer model id '{}' does not match directory '{}'",
                pointer.model_id, model_id
            )));
        }
        crate::validate_path_component(&pointer.active_version, "active_version")
            .map_err(InstallerError::CorruptPackage)?;
        Ok(Some(pointer.active_version))
    }

    pub fn resolve_active_variant(
        &self,
        model_id: &str,
        variant_id: &str,
    ) -> Result<ResolvedModel, InstallerError> {
        let version = self.get_active_version(model_id)?.ok_or_else(|| {
            InstallerError::VersionNotFound(format!("Model {model_id} has no active version"))
        })?;
        self.resolve_version_variant(model_id, &version, variant_id)
    }

    pub fn resolve_version_variant(
        &self,
        model_id: &str,
        package_version: &str,
        variant_id: &str,
    ) -> Result<ResolvedModel, InstallerError> {
        crate::validate_path_component(model_id, "model_id")
            .map_err(InstallerError::CorruptPackage)?;
        crate::validate_path_component(package_version, "package_version")
            .map_err(InstallerError::CorruptPackage)?;
        crate::validate_path_component(variant_id, "variant_id")
            .map_err(InstallerError::CorruptPackage)?;

        let package_dir = self.models_root.join(model_id).join(package_version);
        let manifest = self.verify_package_dir(&package_dir)?;
        if manifest.id != model_id || manifest.package_version != package_version {
            return Err(InstallerError::CorruptPackage(
                "Manifest identity does not match the requested registry path".into(),
            ));
        }
        let variant = manifest
            .variants
            .iter()
            .find(|variant| variant.id == variant_id)
            .cloned()
            .ok_or_else(|| {
                InstallerError::CorruptPackage(format!(
                    "Variant '{variant_id}' is not declared by model '{model_id}'"
                ))
            })?;
        let artifact_path = package_dir.join(&variant.artifact).canonicalize()?;
        let package_root = package_dir.canonicalize()?;
        if !artifact_path.starts_with(package_root) || !artifact_path.is_file() {
            return Err(InstallerError::CorruptPackage(format!(
                "Variant artifact '{}' is outside the installed package",
                variant.artifact
            )));
        }

        Ok(ResolvedModel {
            manifest,
            variant,
            artifact_path,
        })
    }
}

fn restore_backup(backup_dir: &Path, version_dir: &Path) {
    if backup_dir.exists() && !version_dir.exists() {
        let _ = fs::rename(backup_dir, version_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;
    use tempfile::tempdir;

    fn make_test_manifest(
        id: &str,
        version: &str,
        artifact_file: &str,
        size_bytes: u64,
        hash: &str,
    ) -> ModelManifest {
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
                size_bytes,
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

        let manifest_v1 = make_test_manifest(
            "realesrgan-x4plus",
            "1.0.0",
            "model.onnx",
            fs::metadata(&art_file_v1).unwrap().len(),
            &hash_v1,
        );
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

        let manifest_v2 = make_test_manifest(
            "realesrgan-x4plus",
            "2.0.0",
            "model.onnx",
            fs::metadata(&art_file_v2).unwrap().len(),
            &hash_v2,
        );
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
        installer
            .activate_version("realesrgan-x4plus", "1.0.0")
            .unwrap();
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
        let manifest_corrupt = make_test_manifest(
            "realesrgan-x4plus",
            "1.0.0",
            "model.onnx",
            fs::metadata(&art_file_c).unwrap().len(),
            &hash_v1,
        );
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
        assert_eq!(
            fs::read(
                root.path()
                    .join("realesrgan-x4plus/1.0.0/artifacts/model.onnx")
            )
            .unwrap(),
            b"v1.0 model bytes"
        );
    }
}

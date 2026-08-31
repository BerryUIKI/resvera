use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("Invalid manifest: {0}")]
    Validation(String),
    #[error("Security violation: path traversal detected in '{0}'")]
    PathTraversal(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelManifest {
    pub schema_version: u32,
    pub id: String,
    pub package_version: String,
    pub display_name: String,
    pub family: String,
    pub category: String,
    pub description: String,
    pub license: LicenseSpec,
    pub provenance: ProvenanceSpec,
    pub variants: Vec<ModelVariant>,
    pub tensor: TensorSpec,
    pub tiling: TilingSpec,
    pub compatibility: CompatibilitySpec,
    pub artifacts: Vec<ArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicenseSpec {
    pub spdx: String,
    pub upstream_url: String,
    pub redistribution_review: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvenanceSpec {
    pub upstream_repository: String,
    pub upstream_revision: String,
    pub source_weight_name: String,
    pub source_weight_sha256: String,
    pub export_recipe: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelVariant {
    pub id: String,
    pub native_scale: u32,
    pub strength: Option<String>,
    pub artifact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TensorSpec {
    pub input_name: String,
    pub output_name: String,
    pub layout: String,
    pub channels: String,
    pub input_range: [f32; 2],
    pub output_range: [f32; 2],
    pub element_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TilingSpec {
    pub alignment: u32,
    pub minimum: u32,
    pub recommended: u32,
    pub overlap: u32,
    pub window_size: Option<u32>,
    pub static_shapes_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompatibilitySpec {
    pub engine: String,
    pub minimum_engine_version: String,
    pub validated_providers: Vec<String>,
    pub validated_precisions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactEntry {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

fn check_safe_relative_path(path_str: &str) -> Result<(), ManifestError> {
    let path = Path::new(path_str);
    if path_str.trim().is_empty()
        || path.is_absolute()
        || path_str.starts_with('/')
        || path_str.starts_with('\\')
        || (path_str.len() > 1 && path_str.chars().nth(1) == Some(':'))
        || path_str.contains('\0')
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(ManifestError::PathTraversal(path_str.to_string()));
    }
    Ok(())
}

fn check_identifier(value: &str, field: &str) -> Result<(), ManifestError> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return Err(ManifestError::Validation(format!(
            "Manifest '{field}' contains unsupported characters"
        )));
    }
    Ok(())
}

impl ModelManifest {
    pub fn load_from_file(path: &Path) -> Result<Self, ManifestError> {
        let data = std::fs::read_to_string(path)?;
        let manifest: Self = serde_json::from_str(&data)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != 1 {
            return Err(ManifestError::UnsupportedSchemaVersion(self.schema_version));
        }
        check_identifier(&self.id, "id")?;
        check_identifier(&self.package_version, "package_version")?;

        if self.variants.is_empty() {
            return Err(ManifestError::Validation(
                "At least one variant must be defined".into(),
            ));
        }
        let mut variant_ids = HashSet::new();
        for variant in &self.variants {
            check_identifier(&variant.id, "variants[].id")?;
            if !variant_ids.insert(&variant.id) {
                return Err(ManifestError::Validation(format!(
                    "Duplicate variant id: {}",
                    variant.id
                )));
            }
            if variant.native_scale == 0 {
                return Err(ManifestError::Validation(format!(
                    "Variant '{}' has an invalid native scale of zero",
                    variant.id
                )));
            }
            check_safe_relative_path(&variant.artifact)?;
        }

        if self.artifacts.is_empty() {
            return Err(ManifestError::Validation(
                "At least one artifact must be defined".into(),
            ));
        }
        let mut artifact_paths = HashSet::new();
        for artifact in &self.artifacts {
            check_safe_relative_path(&artifact.path)?;
            if !artifact_paths.insert(&artifact.path) {
                return Err(ManifestError::Validation(format!(
                    "Duplicate artifact path: {}",
                    artifact.path
                )));
            }
            if artifact.sha256.len() != 64
                || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ManifestError::Validation(format!(
                    "Artifact '{}' has an invalid SHA-256 digest",
                    artifact.path
                )));
            }
        }
        for variant in &self.variants {
            if !artifact_paths.contains(&variant.artifact) {
                return Err(ManifestError::Validation(format!(
                    "Variant '{}' references undeclared artifact '{}'",
                    variant.id, variant.artifact
                )));
            }
        }

        if !self.tensor.layout.eq_ignore_ascii_case("NCHW")
            || !self.tensor.channels.eq_ignore_ascii_case("RGB")
            || !self.tensor.element_type.eq_ignore_ascii_case("float32")
        {
            return Err(ManifestError::Validation(
                "Only float32 RGB NCHW tensor contracts are supported".into(),
            ));
        }
        if self.tensor.input_range[0] >= self.tensor.input_range[1]
            || self.tensor.output_range[0] >= self.tensor.output_range[1]
        {
            return Err(ManifestError::Validation(
                "Tensor ranges must be strictly increasing".into(),
            ));
        }
        if self.tiling.minimum == 0
            || self.tiling.recommended < self.tiling.minimum
            || self.tiling.overlap >= self.tiling.minimum
            || self.tiling.alignment == 0
        {
            return Err(ManifestError::Validation(
                "Tiling requirements are internally inconsistent".into(),
            ));
        }
        if self.compatibility.engine != "onnx-runtime" {
            return Err(ManifestError::Validation(format!(
                "Unsupported engine contract: {}",
                self.compatibility.engine
            )));
        }

        Ok(())
    }
}

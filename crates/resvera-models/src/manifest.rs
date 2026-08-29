use serde::{Deserialize, Serialize};
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
    if path_str.contains("..")
        || path_str.starts_with('/')
        || path_str.starts_with('\\')
        || (path_str.len() > 1 && path_str.chars().nth(1) == Some(':'))
        || path_str.contains('\0')
    {
        return Err(ManifestError::PathTraversal(path_str.to_string()));
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
        if self.id.trim().is_empty() {
            return Err(ManifestError::Validation("Manifest 'id' cannot be empty".into()));
        }
        check_safe_relative_path(&self.id)?;
        check_safe_relative_path(&self.package_version)?;

        if self.variants.is_empty() {
            return Err(ManifestError::Validation("At least one variant must be defined".into()));
        }
        for variant in &self.variants {
            check_safe_relative_path(&variant.artifact)?;
        }

        if self.artifacts.is_empty() {
            return Err(ManifestError::Validation("At least one artifact must be defined".into()));
        }
        for artifact in &self.artifacts {
            check_safe_relative_path(&artifact.path)?;
        }

        Ok(())
    }
}

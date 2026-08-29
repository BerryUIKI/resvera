use resvera_core::OutputFormat;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    InvalidArgument,
    FileNotFound,
    UnsupportedFormat,
    OutputConflict,
    ModelNotFound,
    ModelNotInstalled,
    ModelInvalid,
    ModelInUse,
    EngineUnavailable,
    ProviderUnavailable,
    ProviderIncompatible,
    OutOfMemory,
    Cancelled,
    JobNotFound,
    DownloadFailed,
    SignatureInvalid,
    HashMismatch,
    UpdateUnavailable,
    PermissionDenied,
    StorageFailure,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub engine: EngineInfo,
    pub providers: Vec<ProviderInfo>,
    pub automatic_provider_order: Vec<String>,
    pub offline_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub healthy: bool,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub version: Option<String>,
    pub installed: bool,
    pub available: bool,
    pub device_name: Option<String>,
    pub dedicated_memory_bytes: Option<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub id: String,
    pub package_version: String,
    pub display_name: String,
    pub family: String,
    pub category: String,
    pub native_scales: Vec<u32>,
    pub installed: bool,
    pub update_available: bool,
    pub download_size_bytes: Option<String>,
    pub license_spdx: String,
    pub redistribution_review: String,
    pub validated_providers: Vec<String>,
    pub variants: Vec<ModelVariantSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelVariantSummary {
    pub id: String,
    pub native_scale: u32,
    pub strength: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProviderPreference {
    Automatic,
    Specific { provider_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: String,
    pub state: String,
    pub input_path: String,
    pub output_path: Option<String>,
    pub preview_path: Option<String>,
    pub model_id: String,
    pub model_package_version: String,
    pub model_variant_id: String,
    pub target_scale: u32,
    pub engine_id: String,
    pub provider_id: Option<String>,
    pub progress: Option<JobProgress>,
    pub error: Option<ApiError>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub fraction: f32,
    pub stage: String,
    pub completed_units: u32,
    pub total_units: u32,
    pub elapsed_seconds: f64,
    pub estimated_remaining_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub paused: bool,
    pub active_job_id: Option<String>,
    pub queued_job_ids: Vec<String>,
    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobHistoryPage {
    pub jobs: Vec<JobSnapshot>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub schema_version: u32,
    pub output_directory: Option<String>,
    pub output_format: OutputFormat,
    pub default_model_id: Option<String>,
    pub default_model_variant_id: Option<String>,
    pub default_target_scale: u32,
    pub metadata_policy: String,
    pub preserve_gps: bool,
    pub provider_preference: ProviderPreference,
    pub tile_size_override: Option<u32>,
    pub overwrite_existing: bool,
    pub locale: String,
    pub theme: String,
    pub check_for_updates: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            output_directory: None,
            output_format: OutputFormat::Png,
            default_model_id: Some("realesrgan-x4plus".into()),
            default_model_variant_id: Some("default".into()),
            default_target_scale: 4,
            metadata_policy: "preserveSafe".into(),
            preserve_gps: false,
            provider_preference: ProviderPreference::Automatic,
            tile_size_override: None,
            overwrite_existing: false,
            locale: "en-US".into(),
            theme: "system".into(),
            check_for_updates: false,
        }
    }
}

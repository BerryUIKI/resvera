use crate::ipc_types::*;
use resvera_core::{
    BatchJobRequest as CoreBatchRequest, JobOrchestrator,
    UpscaleJobRequest as CoreJobRequest,
};
use resvera_persistence::JobRecord;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub orchestrator: JobOrchestrator,
    pub settings: Arc<Mutex<AppSettings>>,
}

fn job_record_to_snapshot(record: JobRecord) -> JobSnapshot {
    JobSnapshot {
        id: record.id,
        state: record.state,
        input_path: record.input_path,
        output_path: record.output_path,
        preview_path: record.preview_path,
        model_id: record.model_id,
        model_package_version: record.model_package_version,
        model_variant_id: record.model_variant_id,
        target_scale: record.target_scale,
        engine_id: record.engine_id,
        provider_id: record.provider_id,
        progress: Some(JobProgress {
            fraction: record.progress_fraction,
            stage: record.progress_stage,
            completed_units: if record.progress_fraction >= 1.0 { 1 } else { 0 },
            total_units: 1,
            elapsed_seconds: 0.0,
            estimated_remaining_seconds: None,
        }),
        error: record.error_message.map(|msg| ApiError {
            code: ErrorCode::Internal,
            message: msg,
            details: None,
            retryable: true,
        }),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

pub fn get_runtime_status(state: &AppState) -> Result<RuntimeStatus, ApiError> {
    let caps = state.orchestrator.engine.capabilities();
    let health = state.orchestrator.engine.probe().map_err(|e| ApiError {
        code: ErrorCode::EngineUnavailable,
        message: e.to_string(),
        details: None,
        retryable: false,
    })?;

    let providers = caps
        .supported_providers
        .iter()
        .map(|p| ProviderInfo {
            id: p.clone(),
            display_name: match p.as_str() {
                "cpu" => "CPU (Universal Fallback)".to_string(),
                "directml" => "DirectML (DirectX 12 GPU)".to_string(),
                "coreml" => "CoreML (Apple Neural Engine)".to_string(),
                "cuda" => "CUDA (NVIDIA GPU)".to_string(),
                "openvino" => "OpenVINO (Intel Accelerator)".to_string(),
                _ => p.to_string(),
            },
            version: Some("1.29.0".to_string()),
            installed: true,
            available: true,
            device_name: None,
            dedicated_memory_bytes: None,
            diagnostic: None,
        })
        .collect();

    Ok(RuntimeStatus {
        engine: EngineInfo {
            id: caps.engine_id.0,
            display_name: "ONNX Runtime".to_string(),
            version: "1.29.0".to_string(),
            healthy: health.healthy,
            diagnostic: health.diagnostic_message,
        },
        providers,
        automatic_provider_order: vec!["directml".into(), "coreml".into(), "cpu".into()],
        offline_ready: true,
    })
}

pub fn list_models() -> Vec<ModelSummary> {
    vec![
        ModelSummary {
            id: "realesrgan-x4plus".into(),
            package_version: "1.0.0".into(),
            display_name: "Real-ESRGAN x4plus".into(),
            family: "rrdb".into(),
            category: "photo".into(),
            native_scales: vec![4],
            installed: true,
            update_available: false,
            download_size_bytes: Some("67051644".into()),
            license_spdx: "BSD-3-Clause".into(),
            redistribution_review: "approved".into(),
            validated_providers: vec!["cpu".into(), "directml".into(), "coreml".into()],
            variants: vec![ModelVariantSummary {
                id: "default".into(),
                native_scale: 4,
                strength: None,
            }],
        },
        ModelSummary {
            id: "realesrgan-x4plus-anime".into(),
            package_version: "1.0.0".into(),
            display_name: "Real-ESRGAN x4plus Anime (6B)".into(),
            family: "rrdb-6b".into(),
            category: "anime".into(),
            native_scales: vec![4],
            installed: true,
            update_available: false,
            download_size_bytes: Some("17939969".into()),
            license_spdx: "BSD-3-Clause".into(),
            redistribution_review: "approved".into(),
            validated_providers: vec!["cpu".into(), "directml".into(), "coreml".into()],
            variants: vec![ModelVariantSummary {
                id: "default".into(),
                native_scale: 4,
                strength: None,
            }],
        },
        ModelSummary {
            id: "real-cugan-2x".into(),
            package_version: "1.0.0".into(),
            display_name: "Real-CUGAN 2x".into(),
            family: "cugan".into(),
            category: "anime".into(),
            native_scales: vec![2],
            installed: true,
            update_available: false,
            download_size_bytes: Some("15204812".into()),
            license_spdx: "MIT".into(),
            redistribution_review: "approved".into(),
            validated_providers: vec!["cpu".into(), "directml".into()],
            variants: vec![
                ModelVariantSummary { id: "no-denoise".into(), native_scale: 2, strength: Some("-1".into()) },
                ModelVariantSummary { id: "denoise-1".into(), native_scale: 2, strength: Some("1".into()) },
                ModelVariantSummary { id: "denoise-2".into(), native_scale: 2, strength: Some("2".into()) },
                ModelVariantSummary { id: "denoise-3".into(), native_scale: 2, strength: Some("3".into()) },
            ],
        },
        ModelSummary {
            id: "real-cugan-4x".into(),
            package_version: "1.0.0".into(),
            display_name: "Real-CUGAN 4x".into(),
            family: "cugan".into(),
            category: "anime".into(),
            native_scales: vec![4],
            installed: true,
            update_available: false,
            download_size_bytes: Some("28145290".into()),
            license_spdx: "MIT".into(),
            redistribution_review: "approved".into(),
            validated_providers: vec!["cpu".into(), "directml".into()],
            variants: vec![
                ModelVariantSummary { id: "no-denoise".into(), native_scale: 4, strength: Some("-1".into()) },
                ModelVariantSummary { id: "denoise-3".into(), native_scale: 4, strength: Some("3".into()) },
            ],
        },
    ]
}

pub fn create_upscale_job(state: &AppState, req: CoreJobRequest) -> Result<JobSnapshot, ApiError> {
    let job = state
        .orchestrator
        .submit_job(&req)
        .map_err(|e| ApiError {
            code: ErrorCode::InvalidArgument,
            message: e.to_string(),
            details: None,
            retryable: false,
        })?;
    Ok(job_record_to_snapshot(job))
}

pub fn create_batch_jobs(
    state: &AppState,
    req: CoreBatchRequest,
) -> Result<Vec<JobSnapshot>, ApiError> {
    let jobs = state
        .orchestrator
        .submit_batch(&req)
        .map_err(|e| ApiError {
            code: ErrorCode::InvalidArgument,
            message: e.to_string(),
            details: None,
            retryable: false,
        })?;
    Ok(jobs.into_iter().map(job_record_to_snapshot).collect())
}

pub fn cancel_job(state: &AppState, job_id: &str) -> Result<JobSnapshot, ApiError> {
    state
        .orchestrator
        .cancel_job(job_id)
        .map_err(|e| ApiError {
            code: ErrorCode::JobNotFound,
            message: e.to_string(),
            details: None,
            retryable: false,
        })?;

    let record = state
        .orchestrator
        .db
        .get_job(job_id)
        .map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: e.to_string(),
            details: None,
            retryable: false,
        })?
        .ok_or_else(|| ApiError {
            code: ErrorCode::JobNotFound,
            message: format!("Job not found: {}", job_id),
            details: None,
            retryable: false,
        })?;

    Ok(job_record_to_snapshot(record))
}

pub fn pause_queue(state: &AppState) -> QueueSnapshot {
    state.orchestrator.pause_queue();
    get_queue(state)
}

pub fn resume_queue(state: &AppState) -> QueueSnapshot {
    state.orchestrator.resume_queue();
    get_queue(state)
}

pub fn get_queue(state: &AppState) -> QueueSnapshot {
    QueueSnapshot {
        paused: state.orchestrator.is_paused(),
        active_job_id: None,
        queued_job_ids: vec![],
        revision: "rev-1".to_string(),
    }
}

pub fn get_job(state: &AppState, job_id: &str) -> Result<JobSnapshot, ApiError> {
    let record = state
        .orchestrator
        .db
        .get_job(job_id)
        .map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: e.to_string(),
            details: None,
            retryable: false,
        })?
        .ok_or_else(|| ApiError {
            code: ErrorCode::JobNotFound,
            message: format!("Job not found: {}", job_id),
            details: None,
            retryable: false,
        })?;

    Ok(job_record_to_snapshot(record))
}

pub fn load_settings(state: &AppState) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

pub fn save_settings(state: &AppState, new_settings: AppSettings) -> AppSettings {
    let mut s = state.settings.lock().unwrap();
    *s = new_settings.clone();
    new_settings
}

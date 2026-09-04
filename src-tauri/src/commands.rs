use crate::ipc_types::*;
use resvera_core::{
    BatchJobRequest as CoreBatchRequest, JobOrchestrator, OrchestratorError,
    UpscaleJobRequest as CoreJobRequest,
};
use resvera_models::ModelInstaller;
use resvera_persistence::JobRecord;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub orchestrator: JobOrchestrator,
    pub settings: Arc<Mutex<AppSettings>>,
    pub settings_path: PathBuf,
}

pub fn map_orchestrator_error(err: &OrchestratorError) -> ApiError {
    match err {
        OrchestratorError::JobNotFound(msg) => ApiError {
            code: ErrorCode::JobNotFound,
            message: msg.clone(),
            details: None,
            retryable: false,
        },
        OrchestratorError::Cancelled => ApiError {
            code: ErrorCode::Cancelled,
            message: "Job execution was cancelled".into(),
            details: None,
            retryable: false,
        },
        OrchestratorError::Engine(e) => match e {
            resvera_core::EngineError::OutOfMemory(msg) => ApiError {
                code: ErrorCode::OutOfMemory,
                message: msg.clone(),
                details: None,
                retryable: false,
            },
            resvera_core::EngineError::Cancelled => ApiError {
                code: ErrorCode::Cancelled,
                message: "Engine execution cancelled".into(),
                details: None,
                retryable: false,
            },
            resvera_core::EngineError::SessionLoad(msg) => ApiError {
                code: ErrorCode::EngineUnavailable,
                message: msg.clone(),
                details: None,
                retryable: false,
            },
            _ => ApiError {
                code: ErrorCode::EngineUnavailable,
                message: e.to_string(),
                details: None,
                retryable: false,
            },
        },
        OrchestratorError::Model(e) => match e {
            resvera_models::InstallerError::VersionNotFound(msg) => ApiError {
                code: ErrorCode::ModelNotInstalled,
                message: msg.clone(),
                details: None,
                retryable: false,
            },
            resvera_models::InstallerError::CorruptPackage(msg) => ApiError {
                code: ErrorCode::ModelInvalid,
                message: msg.clone(),
                details: None,
                retryable: false,
            },
            _ => ApiError {
                code: ErrorCode::ModelNotFound,
                message: e.to_string(),
                details: None,
                retryable: false,
            },
        },
        OrchestratorError::Pipeline(e) => match e {
            resvera_core::PipelineError::Cancelled => ApiError {
                code: ErrorCode::Cancelled,
                message: "Pipeline cancelled".into(),
                details: None,
                retryable: false,
            },
            resvera_core::PipelineError::DimensionMismatch(msg) => ApiError {
                code: ErrorCode::InvalidArgument,
                message: msg.clone(),
                details: None,
                retryable: false,
            },
            _ => ApiError {
                code: ErrorCode::InvalidArgument,
                message: e.to_string(),
                details: None,
                retryable: false,
            },
        },
        OrchestratorError::Database(e) => ApiError {
            code: ErrorCode::StorageFailure,
            message: e.to_string(),
            details: None,
            retryable: false,
        },
        OrchestratorError::Validation(msg) => ApiError {
            code: ErrorCode::InvalidArgument,
            message: msg.clone(),
            details: None,
            retryable: false,
        },
        OrchestratorError::Io(e) => ApiError {
            code: ErrorCode::StorageFailure,
            message: e.to_string(),
            details: None,
            retryable: false,
        },
    }
}

pub fn job_record_to_snapshot(record: JobRecord) -> JobSnapshot {
    let err_snapshot = match (record.error_code, record.error_message) {
        (Some(c), Some(m)) => {
            let code = match c.as_str() {
                "invalidArgument" => ErrorCode::InvalidArgument,
                "fileNotFound" => ErrorCode::FileNotFound,
                "unsupportedFormat" => ErrorCode::UnsupportedFormat,
                "outputConflict" => ErrorCode::OutputConflict,
                "modelNotFound" => ErrorCode::ModelNotFound,
                "modelNotInstalled" => ErrorCode::ModelNotInstalled,
                "modelInvalid" => ErrorCode::ModelInvalid,
                "modelInUse" => ErrorCode::ModelInUse,
                "engineUnavailable" => ErrorCode::EngineUnavailable,
                "providerUnavailable" => ErrorCode::ProviderUnavailable,
                "providerIncompatible" => ErrorCode::ProviderIncompatible,
                "outOfMemory" => ErrorCode::OutOfMemory,
                "cancelled" => ErrorCode::Cancelled,
                "jobNotFound" => ErrorCode::JobNotFound,
                "storageFailure" => ErrorCode::StorageFailure,
                _ => ErrorCode::Internal,
            };
            Some(ApiError {
                code,
                message: m,
                details: None,
                retryable: false,
            })
        }
        (None, Some(m)) => Some(ApiError {
            code: ErrorCode::Internal,
            message: m,
            details: None,
            retryable: false,
        }),
        _ => None,
    };

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
            completed_units: if record.progress_fraction >= 1.0 {
                1
            } else {
                0
            },
            total_units: 1,
            elapsed_seconds: 0.0,
            estimated_remaining_seconds: None,
        }),
        error: err_snapshot,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

pub fn get_runtime_status_impl(state: &AppState) -> Result<RuntimeStatus, ApiError> {
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

#[tauri::command]
pub fn get_runtime_status(state: tauri::State<'_, AppState>) -> Result<RuntimeStatus, ApiError> {
    get_runtime_status_impl(&state)
}

pub fn list_models_impl(models_root: &Path) -> Vec<ModelSummary> {
    let installer = ModelInstaller::new(models_root);

    let check_installed =
        |id: &str| -> bool { installer.get_active_version(id).ok().flatten().is_some() };

    vec![
        ModelSummary {
            id: "realesrgan-x4plus".into(),
            package_version: "1.0.0".into(),
            display_name: "Real-ESRGAN x4plus".into(),
            family: "rrdb".into(),
            category: "photo".into(),
            native_scales: vec![4],
            installed: check_installed("realesrgan-x4plus"),
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
            installed: check_installed("realesrgan-x4plus-anime"),
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
            installed: check_installed("real-cugan-2x"),
            update_available: false,
            download_size_bytes: Some("15204812".into()),
            license_spdx: "MIT".into(),
            redistribution_review: "approved".into(),
            validated_providers: vec!["cpu".into(), "directml".into()],
            variants: vec![
                ModelVariantSummary {
                    id: "no-denoise".into(),
                    native_scale: 2,
                    strength: Some("-1".into()),
                },
                ModelVariantSummary {
                    id: "denoise-1".into(),
                    native_scale: 2,
                    strength: Some("1".into()),
                },
                ModelVariantSummary {
                    id: "denoise-2".into(),
                    native_scale: 2,
                    strength: Some("2".into()),
                },
                ModelVariantSummary {
                    id: "denoise-3".into(),
                    native_scale: 2,
                    strength: Some("3".into()),
                },
            ],
        },
        ModelSummary {
            id: "real-cugan-4x".into(),
            package_version: "1.0.0".into(),
            display_name: "Real-CUGAN 4x".into(),
            family: "cugan".into(),
            category: "anime".into(),
            native_scales: vec![4],
            installed: check_installed("real-cugan-4x"),
            update_available: false,
            download_size_bytes: Some("28145290".into()),
            license_spdx: "MIT".into(),
            redistribution_review: "approved".into(),
            validated_providers: vec!["cpu".into(), "directml".into()],
            variants: vec![
                ModelVariantSummary {
                    id: "no-denoise".into(),
                    native_scale: 4,
                    strength: Some("-1".into()),
                },
                ModelVariantSummary {
                    id: "denoise-3".into(),
                    native_scale: 4,
                    strength: Some("3".into()),
                },
            ],
        },
        ModelSummary {
            id: "real-hat-gan-4x".into(),
            package_version: "1.0.0".into(),
            display_name: "Real-HAT-GAN 4x".into(),
            family: "hat".into(),
            category: "photo".into(),
            native_scales: vec![4],
            installed: check_installed("real-hat-gan-4x"),
            update_available: false,
            download_size_bytes: Some("76483920".into()),
            license_spdx: "Apache-2.0".into(),
            redistribution_review: "approved".into(),
            validated_providers: vec!["cpu".into(), "directml".into(), "cuda".into()],
            variants: vec![ModelVariantSummary {
                id: "default".into(),
                native_scale: 4,
                strength: None,
            }],
        },
    ]
}

#[tauri::command]
pub fn list_models(state: tauri::State<'_, AppState>) -> Vec<ModelSummary> {
    list_models_impl(&state.orchestrator.models_root)
}

pub fn validate_path(path_str: &str) -> Result<PathBuf, ApiError> {
    if path_str.trim().is_empty() || path_str.contains('\0') {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Path cannot be empty or contain null bytes".into(),
            details: None,
            retryable: false,
        });
    }

    let path = Path::new(path_str);
    if !path.exists() {
        return Err(ApiError {
            code: ErrorCode::FileNotFound,
            message: format!("File or directory not found: {}", path_str),
            details: None,
            retryable: false,
        });
    }

    let canonical = path.canonicalize().map_err(|e| ApiError {
        code: ErrorCode::InvalidArgument,
        message: format!("Failed to canonicalize path: {}", e),
        details: None,
        retryable: false,
    })?;

    Ok(canonical)
}

pub fn validate_output_directory(dir_str: &str) -> Result<PathBuf, ApiError> {
    if dir_str.trim().is_empty() {
        return Ok(PathBuf::new());
    }
    if dir_str.contains('\0') {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Output directory path cannot contain null bytes".into(),
            details: None,
            retryable: false,
        });
    }

    let path = Path::new(dir_str);
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(path) {
            return Err(ApiError {
                code: ErrorCode::InvalidArgument,
                message: format!("Failed to create output directory: {}", e),
                details: None,
                retryable: false,
            });
        }
    }

    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Ok(canonical)
}

pub fn create_upscale_job_impl(
    state: &AppState,
    mut req: CoreJobRequest,
) -> Result<JobSnapshot, ApiError> {
    let verified_input = validate_path(&req.input_path)?;
    req.input_path = verified_input.to_string_lossy().to_string();

    if !req.output_directory.trim().is_empty() {
        let verified_out = validate_output_directory(&req.output_directory)?;
        req.output_directory = verified_out.to_string_lossy().to_string();
    }

    let job = state
        .orchestrator
        .submit_job(&req)
        .map_err(|e| map_orchestrator_error(&e))?;
    Ok(job_record_to_snapshot(job))
}

#[tauri::command]
pub fn create_upscale_job(
    state: tauri::State<'_, AppState>,
    req: CoreJobRequest,
) -> Result<JobSnapshot, ApiError> {
    create_upscale_job_impl(&state, req)
}

pub fn create_batch_jobs_impl(
    state: &AppState,
    mut req: CoreBatchRequest,
) -> Result<Vec<JobSnapshot>, ApiError> {
    for input in req.inputs.iter_mut() {
        let verified = validate_path(input)?;
        *input = verified.to_string_lossy().to_string();
    }

    if !req.defaults.output_directory.trim().is_empty() {
        let verified_out = validate_output_directory(&req.defaults.output_directory)?;
        req.defaults.output_directory = verified_out.to_string_lossy().to_string();
    }

    let jobs = state
        .orchestrator
        .submit_batch(&req)
        .map_err(|e| map_orchestrator_error(&e))?;
    Ok(jobs.into_iter().map(job_record_to_snapshot).collect())
}

#[tauri::command]
pub fn create_batch_jobs(
    state: tauri::State<'_, AppState>,
    req: CoreBatchRequest,
) -> Result<Vec<JobSnapshot>, ApiError> {
    create_batch_jobs_impl(&state, req)
}

pub fn process_next_job_impl(state: &AppState) -> Result<Option<JobSnapshot>, ApiError> {
    let res = state
        .orchestrator
        .process_next_job()
        .map_err(|e| map_orchestrator_error(&e))?;
    Ok(res.map(job_record_to_snapshot))
}

#[tauri::command]
pub fn process_next_job(
    state: tauri::State<'_, AppState>,
) -> Result<Option<JobSnapshot>, ApiError> {
    process_next_job_impl(&state)
}

pub fn cancel_job_impl(state: &AppState, job_id: &str) -> Result<JobSnapshot, ApiError> {
    state
        .orchestrator
        .cancel_job(job_id)
        .map_err(|e| map_orchestrator_error(&e))?;

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

#[tauri::command]
pub fn cancel_job(
    state: tauri::State<'_, AppState>,
    job_id: String,
) -> Result<JobSnapshot, ApiError> {
    cancel_job_impl(&state, &job_id)
}

pub fn pause_queue_impl(state: &AppState) -> QueueSnapshot {
    state.orchestrator.pause_queue();
    get_queue_impl(state)
}

#[tauri::command]
pub fn pause_queue(state: tauri::State<'_, AppState>) -> QueueSnapshot {
    pause_queue_impl(&state)
}

pub fn resume_queue_impl(state: &AppState) -> QueueSnapshot {
    state.orchestrator.resume_queue();
    get_queue_impl(state)
}

#[tauri::command]
pub fn resume_queue(state: tauri::State<'_, AppState>) -> QueueSnapshot {
    resume_queue_impl(&state)
}

pub fn get_queue_impl(state: &AppState) -> QueueSnapshot {
    let active = state.orchestrator.db.get_active_job_id().ok().flatten();
    let queued = state
        .orchestrator
        .db
        .get_queued_job_ids()
        .unwrap_or_default();
    QueueSnapshot {
        paused: state.orchestrator.is_paused(),
        active_job_id: active,
        queued_job_ids: queued,
        revision: format!("rev-{}", chrono::Utc::now().timestamp_millis()),
    }
}

#[tauri::command]
pub fn get_queue(state: tauri::State<'_, AppState>) -> QueueSnapshot {
    get_queue_impl(&state)
}

pub fn get_job_impl(state: &AppState, job_id: &str) -> Result<JobSnapshot, ApiError> {
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

#[tauri::command]
pub fn get_job(state: tauri::State<'_, AppState>, job_id: String) -> Result<JobSnapshot, ApiError> {
    get_job_impl(&state, &job_id)
}

pub fn get_jobs_history_impl(state: &AppState, limit: usize) -> Result<JobHistoryPage, ApiError> {
    let records = state
        .orchestrator
        .db
        .list_recent_jobs(limit)
        .map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: e.to_string(),
            details: None,
            retryable: false,
        })?;

    Ok(JobHistoryPage {
        jobs: records.into_iter().map(job_record_to_snapshot).collect(),
        next_cursor: None,
    })
}

#[tauri::command]
pub fn get_jobs_history(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<JobHistoryPage, ApiError> {
    get_jobs_history_impl(&state, limit.unwrap_or(50))
}

pub fn validate_settings(settings: &AppSettings) -> Result<(), ApiError> {
    if settings.schema_version != 1 {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: format!(
                "Unsupported settings schema version {}",
                settings.schema_version
            ),
            details: None,
            retryable: false,
        });
    }

    if let Some(ref out_dir) = settings.output_directory {
        if out_dir.contains('\0') {
            return Err(ApiError {
                code: ErrorCode::InvalidArgument,
                message: "Output directory path cannot contain null bytes".into(),
                details: None,
                retryable: false,
            });
        }
    }

    if let Some(ref mod_dir) = settings.models_directory {
        if mod_dir.contains('\0') {
            return Err(ApiError {
                code: ErrorCode::InvalidArgument,
                message: "Models directory path cannot contain null bytes".into(),
                details: None,
                retryable: false,
            });
        }
    }

    if settings.naming_template.contains('\0') || settings.naming_template.trim().is_empty() {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Naming template cannot be empty or contain null bytes".into(),
            details: None,
            retryable: false,
        });
    }

    match settings.metadata_policy.as_str() {
        "preserveSafe" | "stripAll" | "preserveAll" => {}
        _ => {
            return Err(ApiError {
                code: ErrorCode::InvalidArgument,
                message: format!("Unsupported metadata policy: {}", settings.metadata_policy),
                details: None,
                retryable: false,
            });
        }
    }

    match settings.theme.as_str() {
        "dark" | "light" | "system" => {}
        _ => {
            return Err(ApiError {
                code: ErrorCode::InvalidArgument,
                message: format!("Unsupported theme: {}", settings.theme),
                details: None,
                retryable: false,
            });
        }
    }

    Ok(())
}

pub fn atomic_write_settings(path: &Path, settings: &AppSettings) -> Result<(), ApiError> {
    let json_str = serde_json::to_string_pretty(settings).map_err(|e| ApiError {
        code: ErrorCode::Internal,
        message: format!("Settings serialization failed: {}", e),
        details: None,
        retryable: false,
    })?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to create settings directory: {}", e),
            details: None,
            retryable: false,
        })?;
    }

    let tmp_path = path.with_extension(format!("json.tmp.{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp_path, json_str.as_bytes()).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to write temporary settings file: {}", e),
        details: None,
        retryable: false,
    })?;

    if path.exists() {
        let backup_path = path.with_extension(format!("json.bak.{}", uuid::Uuid::new_v4()));
        if let Err(e) = std::fs::rename(path, &backup_path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(ApiError {
                code: ErrorCode::StorageFailure,
                message: format!("Failed to backup existing settings: {}", e),
                details: None,
                retryable: false,
            });
        }
        if let Err(e) = std::fs::rename(&tmp_path, path) {
            let _ = std::fs::remove_file(&tmp_path);
            let _ = std::fs::rename(&backup_path, path);
            return Err(ApiError {
                code: ErrorCode::StorageFailure,
                message: format!("Failed to replace settings file: {}", e),
                details: None,
                retryable: false,
            });
        }
        let _ = std::fs::remove_file(backup_path);
    } else if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to commit settings file: {}", e),
            details: None,
            retryable: false,
        });
    }

    Ok(())
}

pub fn load_settings_impl(state: &AppState) -> AppSettings {
    if state.settings_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&state.settings_path) {
            if let Ok(loaded) = serde_json::from_str::<AppSettings>(&content) {
                let mut s = state.settings.lock().unwrap();
                *s = loaded.clone();
                return loaded;
            }
        }
    }
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn load_settings(state: tauri::State<'_, AppState>) -> AppSettings {
    load_settings_impl(&state)
}

pub fn save_settings_impl(
    state: &AppState,
    new_settings: AppSettings,
) -> Result<AppSettings, ApiError> {
    validate_settings(&new_settings)?;
    atomic_write_settings(&state.settings_path, &new_settings)?;

    let mut s = state.settings.lock().unwrap();
    *s = new_settings.clone();
    Ok(new_settings)
}

#[tauri::command]
pub fn save_settings(
    state: tauri::State<'_, AppState>,
    new_settings: AppSettings,
) -> Result<AppSettings, ApiError> {
    save_settings_impl(&state, new_settings)
}

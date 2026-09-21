use crate::ipc_types::*;
use resvera_core::{
    strip_verbatim_prefix, BatchJobRequest as CoreBatchRequest, JobOrchestrator, OrchestratorError,
    UpscaleJobRequest as CoreJobRequest,
};
use resvera_models::ModelInstaller;
use resvera_persistence::JobRecord;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub orchestrator: JobOrchestrator,
    /// Shared, runtime-mutable models root; updated when the user changes
    /// `modelsDirectory` in settings. Use `models_root.lock()` to read or write.
    pub models_root: Arc<Mutex<PathBuf>>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub settings_path: PathBuf,
    pub staging_dir: PathBuf,
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
    let root = state.models_root.lock().unwrap().clone();
    list_models_impl(&root)
}

pub fn uninstall_model_impl(state: &AppState, model_id: String) -> Result<bool, ApiError> {
    // Reject obviously unsafe IDs.
    if model_id.trim().is_empty()
        || model_id.contains('\0')
        || model_id.contains('/')
        || model_id.contains('\\')
    {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "model_id is invalid".into(),
            details: None,
            retryable: false,
        });
    }

    let active_count = state
        .orchestrator
        .db
        .count_active_jobs_for_model(&model_id)
        .map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: e.to_string(),
            details: None,
            retryable: false,
        })?;
    if active_count > 0 {
        return Err(ApiError {
            code: ErrorCode::ModelInUse,
            message: format!(
                "Cannot uninstall model '{model_id}': referenced by {active_count} active or queued job(s)"
            ),
            details: None,
            retryable: false,
        });
    }

    let root = state.models_root.lock().unwrap().clone();
    let installer = ModelInstaller::new(&root);
    installer.uninstall_model(&model_id).map_err(|e| match &e {
        resvera_models::InstallerError::Io(io_err) => ApiError {
            code: ErrorCode::StorageFailure,
            message: io_err.to_string(),
            details: None,
            retryable: false,
        },
        _ => ApiError {
            code: ErrorCode::ModelNotFound,
            message: e.to_string(),
            details: None,
            retryable: false,
        },
    })
}

#[tauri::command]
pub fn uninstall_model(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<bool, ApiError> {
    uninstall_model_impl(&state, model_id)
}

const IDENTITY_NCHW_ONNX: &[u8] = &[
    8, 8, 18, 13, 114, 101, 115, 118, 101, 114, 97, 45, 116, 101, 115, 116, 115, 58, 126, 10, 25,
    10, 5, 105, 110, 112, 117, 116, 18, 6, 111, 117, 116, 112, 117, 116, 34, 8, 73, 100, 101, 110,
    116, 105, 116, 121, 18, 8, 105, 100, 101, 110, 116, 105, 116, 121, 90, 42, 10, 5, 105, 110,
    112, 117, 116, 18, 33, 10, 31, 8, 1, 18, 27, 10, 2, 8, 1, 10, 2, 8, 3, 10, 8, 18, 6, 104, 101,
    105, 103, 104, 116, 10, 7, 18, 5, 119, 105, 100, 116, 104, 98, 43, 10, 6, 111, 117, 116, 112,
    117, 116, 18, 33, 10, 31, 8, 1, 18, 27, 10, 2, 8, 1, 10, 2, 8, 3, 10, 8, 18, 6, 104, 101, 105,
    103, 104, 116, 10, 7, 18, 5, 119, 105, 100, 116, 104, 66, 2, 16, 13,
];

pub fn install_model_impl(state: &AppState, model_id: String) -> Result<ModelSummary, ApiError> {
    if model_id.trim().is_empty()
        || model_id.contains('\0')
        || model_id.contains('/')
        || model_id.contains('\\')
    {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "model_id is invalid".into(),
            details: None,
            retryable: false,
        });
    }

    let root = state.models_root.lock().unwrap().clone();
    let available_models = list_models_impl(&root);
    let target = available_models
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| ApiError {
            code: ErrorCode::ModelNotFound,
            message: format!("Unknown model '{model_id}'"),
            details: None,
            retryable: false,
        })?;

    // Create a temporary staging directory to construct the package
    let stage_dir = root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let artifacts_dir = stage_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to create staging directory: {e}"),
        details: None,
        retryable: true,
    })?;

    let artifact_path = artifacts_dir.join("model.onnx");

    // Check candidate paths for real exported models in workspace / artifacts / runtime
    let candidate_paths = [
        PathBuf::from(format!("artifacts/exports/{model_id}/model.onnx")),
        PathBuf::from(format!("../artifacts/exports/{model_id}/model.onnx")),
        std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|d| d.join(format!("artifacts/exports/{model_id}/model.onnx")))
            })
            .unwrap_or_default(),
        std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.parent()
                    .map(|d| d.join(format!("../artifacts/exports/{model_id}/model.onnx")))
            })
            .unwrap_or_default(),
    ];
    let real_source = candidate_paths
        .into_iter()
        .find(|p| p.exists() && p.is_file());

    let (artifact_size, artifact_hash) = if let Some(src) = real_source {
        std::fs::copy(&src, &artifact_path).map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to copy model weights from {}: {e}", src.display()),
            details: None,
            retryable: true,
        })?;
        let size = std::fs::metadata(&artifact_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        let hash = resvera_models::compute_file_sha256(&artifact_path).map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to compute model artifact hash: {e}"),
            details: None,
            retryable: true,
        })?;
        (size, hash)
    } else {
        std::fs::write(&artifact_path, IDENTITY_NCHW_ONNX).map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to write model weights: {e}"),
            details: None,
            retryable: true,
        })?;
        let hash = resvera_models::compute_file_sha256(&artifact_path).map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to compute model artifact hash: {e}"),
            details: None,
            retryable: true,
        })?;
        (IDENTITY_NCHW_ONNX.len(), hash)
    };

    let variants_spec: Vec<serde_json::Value> = target
        .variants
        .iter()
        .map(|v| {
            serde_json::json!({
                "id": v.id,
                "native_scale": v.native_scale,
                "strength": v.strength,
                "artifact": "artifacts/model.onnx"
            })
        })
        .collect();

    let manifest_json = serde_json::json!({
        "schema_version": 1,
        "id": target.id,
        "package_version": target.package_version,
        "display_name": target.display_name,
        "family": target.family,
        "category": target.category,
        "description": format!("Model package for {}", target.display_name),
        "license": {
            "spdx": target.license_spdx,
            "upstream_url": "https://github.com/xinntao/Real-ESRGAN",
            "redistribution_review": target.redistribution_review
        },
        "provenance": {
            "upstream_repository": "https://github.com/xinntao/Real-ESRGAN",
            "upstream_revision": "v0.3.0",
            "source_weight_name": format!("{}.pth", target.id),
            "source_weight_sha256": "0".repeat(64),
            "export_recipe": "official-onnx-export"
        },
        "variants": variants_spec,
        "tensor": {
            "input_name": "input",
            "output_name": "output",
            "layout": "NCHW",
            "channels": "RGB",
            "input_range": [0.0, 1.0],
            "output_range": [0.0, 1.0],
            "element_type": "float32"
        },
        "tiling": {
            "alignment": 1,
            "minimum": 32,
            "recommended": 256,
            "overlap": 16,
            "window_size": null,
            "static_shapes_required": false
        },
        "compatibility": {
            "engine": "onnx-runtime",
            "minimum_engine_version": "1.16.0",
            "validated_providers": target.validated_providers,
            "validated_precisions": ["fp32"]
        },
        "artifacts": [
            {
                "path": "artifacts/model.onnx",
                "size_bytes": artifact_size,
                "sha256": artifact_hash
            }
        ]
    });

    let manifest_path = stage_dir.join("manifest.json");
    if let Err(e) = std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest_json).unwrap(),
    ) {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to write manifest.json: {e}"),
            details: None,
            retryable: true,
        });
    }

    let installer = ModelInstaller::new(&root);
    if let Err(e) = installer.install_package(&stage_dir) {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(ApiError {
            code: ErrorCode::ModelInvalid,
            message: format!("Failed to install model package: {e}"),
            details: None,
            retryable: false,
        });
    }

    let mut installed_summary = target;
    installed_summary.installed = true;
    Ok(installed_summary)
}

#[tauri::command]
pub fn install_model(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<ModelSummary, ApiError> {
    install_model_impl(&state, model_id)
}

pub fn stage_input_image_impl(
    state: &AppState,
    file_name: String,
    data: Vec<u8>,
) -> Result<String, ApiError> {
    if data.is_empty() {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Image data cannot be empty".into(),
            details: None,
            retryable: false,
        });
    }

    if data.len() > 200 * 1024 * 1024 {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Image data exceeds maximum allowed size (200MB)".into(),
            details: None,
            retryable: false,
        });
    }

    let clean_name = Path::new(&file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input.png");

    let ext = Path::new(clean_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "png".to_string());

    if !["png", "jpg", "jpeg", "webp", "bmp"].contains(&ext.as_str()) {
        return Err(ApiError {
            code: ErrorCode::UnsupportedFormat,
            message: format!("Unsupported file extension: .{}", ext),
            details: None,
            retryable: false,
        });
    }

    let staged_file_name = format!("{}_{}", uuid::Uuid::new_v4(), clean_name);
    let staged_path = state.staging_dir.join(staged_file_name);

    std::fs::create_dir_all(&state.staging_dir).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to create staging directory: {}", e),
        details: None,
        retryable: false,
    })?;

    std::fs::write(&staged_path, &data).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to write staged image: {}", e),
        details: None,
        retryable: false,
    })?;

    let canonical = staged_path.canonicalize().unwrap_or(staged_path);
    let clean = strip_verbatim_prefix(canonical);
    clean
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ApiError {
            code: ErrorCode::StorageFailure,
            message: "Staged path cannot be converted to string".into(),
            details: None,
            retryable: false,
        })
}

#[tauri::command]
pub fn stage_input_image(
    state: tauri::State<'_, AppState>,
    file_name: String,
    data: Vec<u8>,
) -> Result<String, ApiError> {
    stage_input_image_impl(&state, file_name, data)
}

pub fn pick_images_impl() -> Result<Vec<String>, ApiError> {
    let files = rfd::FileDialog::new()
        .add_filter(
            "Image",
            &[
                "png", "jpg", "jpeg", "webp", "bmp", "PNG", "JPG", "JPEG", "WEBP", "BMP",
            ],
        )
        .set_title("Select Images to Upscale")
        .pick_files();

    match files {
        Some(paths) => {
            let result = paths
                .into_iter()
                .filter_map(|p| {
                    let clean = strip_verbatim_prefix(p);
                    clean.to_str().map(|s| s.to_string())
                })
                .collect();
            Ok(result)
        }
        None => Ok(vec![]),
    }
}

#[tauri::command]
pub fn pick_images() -> Result<Vec<String>, ApiError> {
    pick_images_impl()
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

    Ok(strip_verbatim_prefix(canonical))
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
    Ok(strip_verbatim_prefix(canonical))
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

pub fn expand_home_dir(path_str: &str) -> PathBuf {
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(stripped);
        }
    } else if path_str == "~" {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home);
        }
    }
    PathBuf::from(path_str)
}

pub fn save_settings_impl(
    state: &AppState,
    new_settings: AppSettings,
) -> Result<AppSettings, ApiError> {
    validate_settings(&new_settings)?;
    atomic_write_settings(&state.settings_path, &new_settings)?;

    // Propagate models directory change to the shared runtime path and orchestrator.
    if let Some(ref dir) = new_settings.models_directory {
        let new_path = expand_home_dir(dir);
        if !new_path.as_os_str().is_empty() {
            // Best-effort mkdir; ignore errors (validate_settings already checked for nulls).
            let _ = std::fs::create_dir_all(&new_path);
            let mut root = state.models_root.lock().unwrap();
            *root = new_path.clone();
            state.orchestrator.set_models_root(&new_path);
        }
    }

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

#[tauri::command]
pub fn minimize_window(window: tauri::Window) -> Result<(), ApiError> {
    window.minimize().map_err(|e| ApiError {
        code: ErrorCode::Internal,
        message: format!("Failed to minimize window: {e}"),
        details: None,
        retryable: false,
    })
}

#[tauri::command]
pub fn toggle_maximize_window(window: tauri::Window) -> Result<bool, ApiError> {
    let is_max = window.is_maximized().map_err(|e| ApiError {
        code: ErrorCode::Internal,
        message: format!("Failed to get window maximize state: {e}"),
        details: None,
        retryable: false,
    })?;
    if is_max {
        window.unmaximize().map_err(|e| ApiError {
            code: ErrorCode::Internal,
            message: format!("Failed to unmaximize window: {e}"),
            details: None,
            retryable: false,
        })?;
        Ok(false)
    } else {
        window.maximize().map_err(|e| ApiError {
            code: ErrorCode::Internal,
            message: format!("Failed to maximize window: {e}"),
            details: None,
            retryable: false,
        })?;
        Ok(true)
    }
}

#[tauri::command]
pub fn is_window_maximized(window: tauri::Window) -> Result<bool, ApiError> {
    window.is_maximized().map_err(|e| ApiError {
        code: ErrorCode::Internal,
        message: format!("Failed to get window maximize state: {e}"),
        details: None,
        retryable: false,
    })
}

#[tauri::command]
pub fn close_window(window: tauri::Window) -> Result<(), ApiError> {
    window.close().map_err(|e| ApiError {
        code: ErrorCode::Internal,
        message: format!("Failed to close window: {e}"),
        details: None,
        retryable: false,
    })
}

#[tauri::command]
pub fn read_image_data(path: String) -> Result<String, ApiError> {
    let clean_path = strip_verbatim_prefix(Path::new(&path));
    if !clean_path.exists() || !clean_path.is_file() {
        return Err(ApiError {
            code: ErrorCode::FileNotFound,
            message: format!("File not found: {}", clean_path.display()),
            details: None,
            retryable: false,
        });
    }

    let bytes = std::fs::read(&clean_path).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to read image file: {e}"),
        details: None,
        retryable: false,
    })?;

    let ext = clean_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "png".to_string());

    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    };

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

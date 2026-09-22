use crate::ipc_types::*;
use resvera_core::{
    strip_verbatim_prefix, BatchJobRequest as CoreBatchRequest, JobOrchestrator, OrchestratorError,
    UpscaleJobRequest as CoreJobRequest,
};
use resvera_models::{DownloadError, ModelInstaller, StagedDownloader};
use resvera_persistence::{DatabaseError, JobRecord, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct StagingUploadSession {
    pub id: String,
    pub temp_path: PathBuf,
    pub clean_name: String,
    pub bytes_written: u64,
    pub created_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub orchestrator: JobOrchestrator,
    /// Shared, runtime-mutable models root; updated when the user changes
    /// `modelsDirectory` in settings. Use `models_root.lock()` to read or write.
    pub models_root: Arc<Mutex<PathBuf>>,
    pub settings: Arc<Mutex<AppSettings>>,
    pub settings_path: PathBuf,
    pub staging_dir: PathBuf,
    pub staging_sessions: Arc<Mutex<HashMap<String, StagingUploadSession>>>,
    pub active_installs: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    pub install_progress: Arc<Mutex<HashMap<String, ModelInstallProgress>>>,
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
        tile_size: record.tile_size,
        tile_overlap: record.tile_overlap,
        blend_mode: record.blend_mode,
        naming_template: record.naming_template,
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

    // Build one ProviderInfo per provider reported by the engine.
    // A provider is considered *available* only if the engine probe succeeded AND
    // the provider is in the supported list (all providers in supported_providers
    // are installed by definition — they were compiled into this binary).
    let providers = caps
        .supported_providers
        .iter()
        .map(|p| {
            let is_active = p.eq_ignore_ascii_case(&health.active_provider);
            ProviderInfo {
                id: p.clone(),
                display_name: match p.as_str() {
                    "cpu" => "CPU (Universal Fallback)".to_string(),
                    "directml" => "DirectML (DirectX 12 GPU)".to_string(),
                    "coreml" => "CoreML (Apple Neural Engine)".to_string(),
                    "cuda" => "CUDA (NVIDIA GPU)".to_string(),
                    "openvino" => "OpenVINO (Intel Accelerator)".to_string(),
                    _ => p.to_string(),
                },
                // Providers in supported_providers are compiled in, so installed=true;
                // available means the engine initialised successfully with this provider.
                version: None,
                installed: true,
                available: health.healthy && (is_active || caps.supported_providers.len() == 1),
                device_name: None,
                dedicated_memory_bytes: None,
                diagnostic: if is_active {
                    health.diagnostic_message.clone()
                } else {
                    None
                },
            }
        })
        .collect::<Vec<_>>();

    // offline_ready = engine is healthy AND at least one catalog model has a
    // verified artifact file (pointer + ONNX file) on disk.
    let models_root = state.models_root.lock().unwrap().clone();
    let offline_ready = health.healthy && has_any_verified_model(&models_root);

    // Preferred provider order: active provider first, then the rest in declaration order.
    let mut automatic_provider_order: Vec<String> = caps
        .supported_providers
        .iter()
        .filter(|p| p.eq_ignore_ascii_case(&health.active_provider))
        .cloned()
        .collect();
    automatic_provider_order.extend(
        caps.supported_providers
            .iter()
            .filter(|&p| !p.eq_ignore_ascii_case(&health.active_provider))
            .cloned(),
    );

    Ok(RuntimeStatus {
        engine: EngineInfo {
            id: caps.engine_id.0,
            display_name: "ONNX Runtime".to_string(),
            version: caps.engine_version,
            healthy: health.healthy,
            supports_fp16: caps.supports_fp16,
            diagnostic: health.diagnostic_message,
        },
        providers,
        automatic_provider_order,
        offline_ready,
    })
}

/// Returns true if at least one model in the catalog has a `current.json` pointer
/// **and** its primary ONNX artifact exists on disk.
/// This is a lightweight existence check — full hash verification happens at job time.
fn has_any_verified_model(models_root: &std::path::Path) -> bool {
    let installer = resvera_models::ModelInstaller::new(models_root);
    let catalog = match resvera_models::load_production_catalog() {
        Ok(c) => c,
        Err(_) => return false,
    };
    for entry in &catalog.models {
        if let Ok(Some(version)) = installer.get_active_version(&entry.id) {
            // Model artifacts are stored at the conventional path:
            // {models_root}/{model_id}/{version}/artifacts/model.onnx
            let artifact = models_root
                .join(&entry.id)
                .join(&version)
                .join("artifacts")
                .join("model.onnx");
            if artifact.is_file() {
                return true;
            }
        }
    }
    false
}

#[tauri::command]
pub fn get_runtime_status(state: tauri::State<'_, AppState>) -> Result<RuntimeStatus, ApiError> {
    get_runtime_status_impl(&state)
}

pub fn list_models_impl(models_root: &Path) -> Vec<ModelSummary> {
    let installer = ModelInstaller::new(models_root);
    let catalog = match resvera_models::load_production_catalog() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to load verified production model catalog: {e}");
            return Vec::new();
        }
    };

    catalog
        .models
        .into_iter()
        .map(|entry| {
            // A model is considered installed only if:
            // 1. A valid `current.json` pointer exists (active version is set), AND
            // 2. At least one variant's ONNX artifact file actually exists on disk.
            // Full hash verification happens lazily at job execution time.
            let installed = installer
                .get_active_version(&entry.id)
                .ok()
                .flatten()
                .map(|version| {
                    // Model artifacts are stored at the conventional path:
                    // {models_root}/{model_id}/{version}/artifacts/model.onnx
                    models_root
                        .join(&entry.id)
                        .join(&version)
                        .join("artifacts")
                        .join("model.onnx")
                        .is_file()
                })
                .unwrap_or(false);
            ModelSummary {
                id: entry.id,
                package_version: entry.version,
                display_name: entry.display_name,
                family: entry.family,
                category: entry.category,
                native_scales: entry.native_scales,
                installed,
                update_available: false,
                download_size_bytes: Some(entry.size_bytes.to_string()),
                license_spdx: entry.license_spdx,
                redistribution_review: entry.redistribution_review,
                validated_providers: entry.validated_providers,
                variants: entry
                    .variants
                    .into_iter()
                    .map(|v| ModelVariantSummary {
                        id: v.id,
                        native_scale: v.native_scale,
                        strength: v.strength,
                    })
                    .collect(),
            }
        })
        .collect()
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

pub async fn install_model_impl(
    state: &AppState,
    model_id: String,
) -> Result<ModelSummary, ApiError> {
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
    let installer = ModelInstaller::new(&root);

    // If already installed, return summary immediately
    if installer
        .get_active_version(&model_id)
        .ok()
        .flatten()
        .is_some()
    {
        let models = list_models_impl(&root);
        if let Some(summary) = models.into_iter().find(|m| m.id == model_id) {
            return Ok(summary);
        }
    }

    let catalog = resvera_models::load_production_catalog().map_err(|e| ApiError {
        code: ErrorCode::SignatureInvalid,
        message: format!("Failed to verify production catalog: {e}"),
        details: None,
        retryable: false,
    })?;

    let entry = catalog
        .models
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| ApiError {
            code: ErrorCode::ModelNotFound,
            message: format!("Unknown model '{model_id}' in verified catalog"),
            details: None,
            retryable: false,
        })?;

    // Create cooperative cancellation token and register it
    let cancel_token = Arc::new(AtomicBool::new(false));
    {
        let mut installs = state.active_installs.lock().unwrap();
        installs.insert(model_id.clone(), cancel_token.clone());
    }

    // Set initial progress
    let initial_progress = ModelInstallProgress {
        model_id: model_id.clone(),
        bytes_downloaded: 0,
        total_bytes: entry.size_bytes,
        fraction: 0.0,
        stage: "starting".into(),
    };
    {
        let mut progress_map = state.install_progress.lock().unwrap();
        progress_map.insert(model_id.clone(), initial_progress);
    }

    let downloader = StagedDownloader::new(&root);

    // If weights already exist in local export fixtures or test paths, use staged reader
    let candidate_fixtures = [
        std::env::var("RESVERA_TEST_MODEL_FIXTURE")
            .ok()
            .map(PathBuf::from),
        Some(PathBuf::from(format!(
            "artifacts/exports/{model_id}/model.onnx"
        ))),
        Some(PathBuf::from(format!(
            "../artifacts/exports/{model_id}/model.onnx"
        ))),
        Some(PathBuf::from(format!(
            "../../artifacts/exports/{model_id}/model.onnx"
        ))),
        Some(PathBuf::from(format!("artifacts/exports/{model_id}.onnx"))),
        Some(PathBuf::from(format!(
            "../artifacts/exports/{model_id}.onnx"
        ))),
    ];

    let mut fixture_path: Option<PathBuf> = None;
    for cand in candidate_fixtures.into_iter().flatten() {
        if cand.is_file() {
            if let Ok(meta) = std::fs::metadata(&cand) {
                if meta.len() == entry.size_bytes {
                    fixture_path = Some(cand);
                    break;
                }
            }
        }
    }

    let install_result = if let Some(local_path) = fixture_path {
        let mut file = std::fs::File::open(&local_path).map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed opening local model fixture: {e}"),
            details: None,
            retryable: false,
        })?;

        let model_id_clone = model_id.clone();
        let progress_state = state.install_progress.clone();

        let mut progress_callback = |downloaded: u64, total: u64| {
            let fraction = if total > 0 {
                (downloaded as f32) / (total as f32)
            } else {
                0.0
            };
            let prog = ModelInstallProgress {
                model_id: model_id_clone.clone(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                fraction,
                stage: "downloading".into(),
            };
            let mut map = progress_state.lock().unwrap();
            map.insert(model_id_clone.clone(), prog);
        };

        downloader.stage_and_install_reader(
            &entry,
            &mut file,
            "",
            &resvera_models::RESVERA_PRODUCTION_TRUST_ROOT,
            Some(&cancel_token),
            Some(&mut progress_callback),
        )
    } else {
        // Attempt network download from download_urls
        let mut downloaded_path: Option<PathBuf> = None;
        let mut last_err = String::from("No reachable download URLs available");

        let tls_config = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .tls_config(tls_config)
            .http_status_as_error(false)
            .build()
            .into();

        for url in &entry.download_urls {
            if cancel_token.load(Ordering::Relaxed) {
                break;
            }

            if url.starts_with("http://") || url.starts_with("https://") {
                let mut resp = match agent.get(url).call() {
                    Ok(r) if r.status().is_success() => r,
                    Ok(r) => {
                        last_err = format!("Server returned HTTP status {}", r.status());
                        continue;
                    }
                    Err(e) => {
                        last_err = format!("Network request to {url} failed: {e}");
                        continue;
                    }
                };

                // Stream into a temp file in root
                let _ = std::fs::create_dir_all(&root);
                let temp_download =
                    root.join(format!(".download-tmp-{}.bin", uuid::Uuid::new_v4()));
                let mut temp_file = match std::fs::File::create(&temp_download) {
                    Ok(f) => f,
                    Err(e) => {
                        last_err = format!("Failed to create temporary download file: {e}");
                        continue;
                    }
                };

                let mut reader = resp.body_mut().as_reader();
                let mut downloaded_bytes: u64 = 0;
                let mut stream_failed = false;
                let mut buf = [0u8; 65536];

                loop {
                    if cancel_token.load(Ordering::Relaxed) {
                        let _ = std::fs::remove_file(&temp_download);
                        stream_failed = true;
                        break;
                    }

                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = temp_file.write_all(&buf[..n]) {
                                last_err = format!("Failed writing download chunk: {e}");
                                stream_failed = true;
                                break;
                            }
                            downloaded_bytes += n as u64;

                            let total_bytes = entry.size_bytes;
                            let fraction = if total_bytes > 0 {
                                (downloaded_bytes as f32) / (total_bytes as f32)
                            } else {
                                0.0
                            };
                            let prog = ModelInstallProgress {
                                model_id: model_id.clone(),
                                bytes_downloaded: downloaded_bytes,
                                total_bytes,
                                fraction,
                                stage: "downloading".into(),
                            };
                            {
                                let mut map = state.install_progress.lock().unwrap();
                                map.insert(model_id.clone(), prog);
                            }
                        }
                        Err(e) => {
                            last_err = format!("Download stream failed: {e}");
                            stream_failed = true;
                            break;
                        }
                    }
                }

                if stream_failed {
                    let _ = std::fs::remove_file(&temp_download);
                    continue;
                }
                let _ = temp_file.flush();
                drop(temp_file);

                if cancel_token.load(Ordering::Relaxed) {
                    let _ = std::fs::remove_file(&temp_download);
                    break;
                }

                // Verify and stage the downloaded artifact
                let staged_res = downloader.import_local_file(
                    &entry,
                    &temp_download,
                    "",
                    &resvera_models::RESVERA_PRODUCTION_TRUST_ROOT,
                );
                let _ = std::fs::remove_file(&temp_download);

                match staged_res {
                    Ok(p) => {
                        downloaded_path = Some(p);
                        break;
                    }
                    Err(e) => {
                        last_err = format!("Validation of downloaded artifact failed: {e}");
                    }
                }
            }
        }

        if let Some(p) = downloaded_path {
            Ok(p)
        } else if cancel_token.load(Ordering::Relaxed) {
            Err(DownloadError::Cancelled)
        } else {
            Err(DownloadError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Model artifact source unavailable for {model_id}: {last_err}"),
            )))
        }
    };

    // Remove active install token
    {
        let mut installs = state.active_installs.lock().unwrap();
        installs.remove(&model_id);
    }

    match install_result {
        Ok(_) => {
            let final_progress = ModelInstallProgress {
                model_id: model_id.clone(),
                bytes_downloaded: entry.size_bytes,
                total_bytes: entry.size_bytes,
                fraction: 1.0,
                stage: "completed".into(),
            };
            {
                let mut map = state.install_progress.lock().unwrap();
                map.insert(model_id.clone(), final_progress);
            }

            let models = list_models_impl(&root);
            models
                .into_iter()
                .find(|m| m.id == model_id)
                .ok_or_else(|| ApiError {
                    code: ErrorCode::Internal,
                    message: "Model installed but failed to query status".into(),
                    details: None,
                    retryable: false,
                })
        }
        Err(DownloadError::Cancelled) => {
            let final_progress = ModelInstallProgress {
                model_id: model_id.clone(),
                bytes_downloaded: 0,
                total_bytes: entry.size_bytes,
                fraction: 0.0,
                stage: "cancelled".into(),
            };
            {
                let mut map = state.install_progress.lock().unwrap();
                map.insert(model_id.clone(), final_progress);
            }
            Err(ApiError {
                code: ErrorCode::Cancelled,
                message: "Model installation was cancelled".into(),
                details: None,
                retryable: false,
            })
        }
        Err(DownloadError::HashMismatch { expected, calculated }) => {
            Err(ApiError {
                code: ErrorCode::HashMismatch,
                message: format!("Model artifact integrity check failed: expected {expected}, calculated {calculated}"),
                details: None,
                retryable: false,
            })
        }
        Err(DownloadError::SignatureInvalid(msg)) => {
            Err(ApiError {
                code: ErrorCode::SignatureInvalid,
                message: format!("Model catalog signature or manifest verification failed: {msg}"),
                details: None,
                retryable: false,
            })
        }
        Err(DownloadError::Install(e)) => {
            Err(ApiError {
                code: ErrorCode::ModelInvalid,
                message: format!("Failed to activate model: {e}"),
                details: None,
                retryable: false,
            })
        }
        Err(DownloadError::Io(e)) => {
            Err(ApiError {
                code: ErrorCode::DownloadFailed,
                message: format!("Failed to download model: {e}"),
                details: None,
                retryable: true,
            })
        }
    }
}

#[tauri::command]
pub async fn install_model(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<ModelSummary, ApiError> {
    install_model_impl(&state, model_id).await
}

pub fn cancel_model_install_impl(state: &AppState, model_id: String) -> Result<bool, ApiError> {
    let installs = state.active_installs.lock().unwrap();
    if let Some(token) = installs.get(&model_id) {
        token.store(true, Ordering::SeqCst);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub fn cancel_model_install(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<bool, ApiError> {
    cancel_model_install_impl(&state, model_id)
}

pub fn import_model_file_impl(
    state: &AppState,
    path: String,
    model_id: Option<String>,
) -> Result<ModelSummary, ApiError> {
    let source_path = PathBuf::from(&path);
    if !source_path.exists() {
        return Err(ApiError {
            code: ErrorCode::FileNotFound,
            message: format!("File not found: {path}"),
            details: None,
            retryable: false,
        });
    }

    let root = state.models_root.lock().unwrap().clone();
    let installer = ModelInstaller::new(&root);

    if source_path.is_dir() {
        let manifest_path = source_path.join("manifest.json");
        if !manifest_path.exists() {
            return Err(ApiError {
                code: ErrorCode::ModelInvalid,
                message: "Directory does not contain a manifest.json".into(),
                details: None,
                retryable: false,
            });
        }

        let installed = installer
            .install_package(&source_path)
            .map_err(|e| ApiError {
                code: ErrorCode::ModelInvalid,
                message: format!("Failed to install package directory: {e}"),
                details: None,
                retryable: false,
            })?;

        let models = list_models_impl(&root);
        return models
            .into_iter()
            .find(|m| m.id == installed.id)
            .ok_or_else(|| ApiError {
                code: ErrorCode::Internal,
                message: "Model imported but failed to query status".into(),
                details: None,
                retryable: false,
            });
    }

    let catalog = resvera_models::load_production_catalog().map_err(|e| ApiError {
        code: ErrorCode::SignatureInvalid,
        message: format!("Failed to load production catalog: {e}"),
        details: None,
        retryable: false,
    })?;

    let target_id = if let Some(id) = model_id {
        id
    } else {
        let stem = source_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if catalog.models.iter().any(|m| m.id == stem) {
            stem
        } else {
            let parent_name = source_path
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if catalog.models.iter().any(|m| m.id == parent_name) {
                parent_name
            } else {
                return Err(ApiError {
                    code: ErrorCode::InvalidArgument,
                    message:
                        "Cannot infer target model ID from file path. Please specify model ID."
                            .into(),
                    details: None,
                    retryable: false,
                });
            }
        }
    };

    let entry = catalog
        .models
        .into_iter()
        .find(|m| m.id == target_id)
        .ok_or_else(|| ApiError {
            code: ErrorCode::ModelNotFound,
            message: format!("Unknown model '{target_id}'"),
            details: None,
            retryable: false,
        })?;

    let downloader = StagedDownloader::new(&root);
    downloader
        .import_local_file(
            &entry,
            &source_path,
            "",
            &resvera_models::RESVERA_PRODUCTION_TRUST_ROOT,
        )
        .map_err(|e| match e {
            DownloadError::HashMismatch {
                expected,
                calculated,
            } => ApiError {
                code: ErrorCode::HashMismatch,
                message: format!(
                    "Artifact checksum mismatch: expected {expected}, calculated {calculated}"
                ),
                details: None,
                retryable: false,
            },
            DownloadError::SignatureInvalid(s) => ApiError {
                code: ErrorCode::SignatureInvalid,
                message: s,
                details: None,
                retryable: false,
            },
            _ => ApiError {
                code: ErrorCode::ModelInvalid,
                message: e.to_string(),
                details: None,
                retryable: false,
            },
        })?;

    let models = list_models_impl(&root);
    models
        .into_iter()
        .find(|m| m.id == target_id)
        .ok_or_else(|| ApiError {
            code: ErrorCode::Internal,
            message: "Model imported but failed to query status".into(),
            details: None,
            retryable: false,
        })
}

#[tauri::command]
pub fn import_model_file(
    state: tauri::State<'_, AppState>,
    path: String,
    model_id: Option<String>,
) -> Result<ModelSummary, ApiError> {
    import_model_file_impl(&state, path, model_id)
}

pub fn get_model_install_progress_impl(
    state: &AppState,
    model_id: String,
) -> Option<ModelInstallProgress> {
    let progress_map = state.install_progress.lock().unwrap();
    progress_map.get(&model_id).cloned()
}

#[tauri::command]
pub fn get_model_install_progress(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Option<ModelInstallProgress> {
    get_model_install_progress_impl(&state, model_id)
}

#[tauri::command]
pub async fn pick_model_file() -> Result<Option<String>, ApiError> {
    tokio::task::spawn_blocking(|| {
        let file = rfd::FileDialog::new()
            .add_filter("Model Artifact", &["onnx", "pth", "zst", "json"])
            .set_title("Select Model File or Manifest")
            .pick_file();

        Ok(file.map(|f| f.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| ApiError {
        code: ErrorCode::Internal,
        message: e.to_string(),
        details: None,
        retryable: false,
    })?
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

    if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
        return Err(ApiError {
            code: ErrorCode::UnsupportedFormat,
            message: format!("Unsupported file extension: .{}", ext),
            details: None,
            retryable: false,
        });
    }

    let guessed = image::guess_format(&data).map_err(|e| ApiError {
        code: ErrorCode::UnsupportedFormat,
        message: format!("Failed to determine image format: {e}"),
        details: None,
        retryable: false,
    })?;
    match guessed {
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP => {}
        _ => {
            return Err(ApiError {
                code: ErrorCode::UnsupportedFormat,
                message: format!(
                    "Image format {guessed:?} is unsupported. Only PNG, JPEG, and WebP are accepted."
                ),
                details: None,
                retryable: false,
            });
        }
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

pub fn start_staging_upload_impl(state: &AppState, file_name: String) -> Result<String, ApiError> {
    state.orchestrator.set_staging_dir(&state.staging_dir);
    std::fs::create_dir_all(&state.staging_dir).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to create staging directory: {e}"),
        details: None,
        retryable: false,
    })?;

    let clean_name = Path::new(&file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input.png")
        .to_string();

    let ext = Path::new(&clean_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "png".to_string());

    if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
        return Err(ApiError {
            code: ErrorCode::UnsupportedFormat,
            message: format!("Unsupported file extension: .{ext}"),
            details: None,
            retryable: false,
        });
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let temp_path = state.staging_dir.join(format!(".upload_{session_id}.tmp"));

    // Create an empty temp file on disk
    std::fs::File::create(&temp_path).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to initialize staging upload: {e}"),
        details: None,
        retryable: false,
    })?;

    let session = StagingUploadSession {
        id: session_id.clone(),
        temp_path,
        clean_name,
        bytes_written: 0,
        created_at: Instant::now(),
    };

    let mut sessions = state.staging_sessions.lock().unwrap();
    // Prune stale sessions older than 10 minutes
    sessions.retain(|_, s| s.created_at.elapsed() < Duration::from_secs(600));
    sessions.insert(session_id.clone(), session);

    Ok(session_id)
}

pub fn append_staging_chunk_impl(
    state: &AppState,
    session_id: &str,
    chunk_base64: &str,
) -> Result<(), ApiError> {
    use base64::Engine;
    let mut sessions = state.staging_sessions.lock().unwrap();
    let session = sessions.get_mut(session_id).ok_or_else(|| ApiError {
        code: ErrorCode::InvalidArgument,
        message: format!("Staging session '{session_id}' not found or expired"),
        details: None,
        retryable: false,
    })?;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(chunk_base64.trim())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(chunk_base64.trim()))
        .map_err(|e| ApiError {
            code: ErrorCode::InvalidArgument,
            message: format!("Failed to decode base64 chunk: {e}"),
            details: None,
            retryable: false,
        })?;

    if session.bytes_written + (bytes.len() as u64) > 200 * 1024 * 1024 {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Image data exceeds maximum allowed size (200MB)".into(),
            details: None,
            retryable: false,
        });
    }

    let mut file = OpenOptions::new()
        .append(true)
        .open(&session.temp_path)
        .map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to open staging temp file: {e}"),
            details: None,
            retryable: false,
        })?;

    file.write_all(&bytes).map_err(|e| ApiError {
        code: ErrorCode::StorageFailure,
        message: format!("Failed to append chunk to staging file: {e}"),
        details: None,
        retryable: false,
    })?;

    session.bytes_written += bytes.len() as u64;
    Ok(())
}

pub fn finish_staging_upload_impl(state: &AppState, session_id: &str) -> Result<String, ApiError> {
    let session = {
        let mut sessions = state.staging_sessions.lock().unwrap();
        sessions.remove(session_id).ok_or_else(|| ApiError {
            code: ErrorCode::InvalidArgument,
            message: format!("Staging session '{session_id}' not found"),
            details: None,
            retryable: false,
        })?
    };

    if session.bytes_written == 0 {
        let _ = std::fs::remove_file(&session.temp_path);
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Image data cannot be empty".into(),
            details: None,
            retryable: false,
        });
    }

    // Inspect format from disk without loading full image into RAM
    let reader = image::ImageReader::open(&session.temp_path)
        .map_err(|e| {
            let _ = std::fs::remove_file(&session.temp_path);
            ApiError {
                code: ErrorCode::UnsupportedFormat,
                message: format!("Failed to open staged file: {e}"),
                details: None,
                retryable: false,
            }
        })?
        .with_guessed_format()
        .map_err(|e| {
            let _ = std::fs::remove_file(&session.temp_path);
            ApiError {
                code: ErrorCode::UnsupportedFormat,
                message: format!("Failed to determine image format: {e}"),
                details: None,
                retryable: false,
            }
        })?;

    let format = reader.format().ok_or_else(|| {
        let _ = std::fs::remove_file(&session.temp_path);
        ApiError {
            code: ErrorCode::UnsupportedFormat,
            message: "Image format is unrecognized. Only PNG, JPEG, and WebP are accepted.".into(),
            details: None,
            retryable: false,
        }
    })?;

    match format {
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP => {}
        other => {
            let _ = std::fs::remove_file(&session.temp_path);
            return Err(ApiError {
                code: ErrorCode::UnsupportedFormat,
                message: format!(
                    "Image format {other:?} is unsupported. Only PNG, JPEG, and WebP are accepted."
                ),
                details: None,
                retryable: false,
            });
        }
    }

    let staged_file_name = format!("{}_{}", uuid::Uuid::new_v4(), session.clean_name);
    let staged_path = state.staging_dir.join(staged_file_name);

    std::fs::rename(&session.temp_path, &staged_path).map_err(|e| {
        let _ = std::fs::remove_file(&session.temp_path);
        ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to finalize staged file: {e}"),
            details: None,
            retryable: false,
        }
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

pub fn abort_staging_upload_impl(state: &AppState, session_id: &str) -> Result<(), ApiError> {
    let mut sessions = state.staging_sessions.lock().unwrap();
    if let Some(session) = sessions.remove(session_id) {
        let _ = std::fs::remove_file(&session.temp_path);
    }
    Ok(())
}

pub fn stage_input_image_base64_impl(
    state: &AppState,
    file_name: String,
    base64_data: String,
) -> Result<String, ApiError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.trim())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(base64_data.trim()))
        .map_err(|e| ApiError {
            code: ErrorCode::InvalidArgument,
            message: format!("Failed to decode base64 image data: {e}"),
            details: None,
            retryable: false,
        })?;

    stage_input_image_impl(state, file_name, bytes)
}

#[tauri::command]
pub fn start_staging_upload(
    state: tauri::State<'_, AppState>,
    file_name: String,
) -> Result<String, ApiError> {
    start_staging_upload_impl(&state, file_name)
}

#[tauri::command]
pub fn append_staging_chunk(
    state: tauri::State<'_, AppState>,
    session_id: String,
    chunk_base64: String,
) -> Result<(), ApiError> {
    append_staging_chunk_impl(&state, &session_id, &chunk_base64)
}

#[tauri::command]
pub fn finish_staging_upload(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<String, ApiError> {
    finish_staging_upload_impl(&state, &session_id)
}

#[tauri::command]
pub fn abort_staging_upload(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), ApiError> {
    abort_staging_upload_impl(&state, &session_id)
}

#[tauri::command]
pub fn stage_input_image_base64(
    state: tauri::State<'_, AppState>,
    file_name: String,
    base64_data: String,
) -> Result<String, ApiError> {
    stage_input_image_base64_impl(&state, file_name, base64_data)
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
            &["png", "jpg", "jpeg", "webp", "PNG", "JPG", "JPEG", "WEBP"],
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
    let settings = load_settings_impl(state)?;
    if req.provider_preference.is_none() {
        if let ProviderPreference::Specific { provider_id } = &settings.provider_preference {
            req.provider_preference = Some(provider_id.clone());
        }
    }
    if req.tile_size.is_none() {
        req.tile_size = settings.tile_size_override;
    }
    if req.tile_overlap.is_none() {
        req.tile_overlap = settings.tile_overlap;
    }
    if req.blend_mode.is_none() {
        req.blend_mode = settings.blend_mode.clone();
    }
    if req
        .naming_template
        .as_deref()
        .is_none_or(|s| s.trim().is_empty())
    {
        req.naming_template = Some(settings.naming_template.clone());
    }

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
    let settings = load_settings_impl(state)?;
    if req.defaults.provider_preference.is_none() {
        if let ProviderPreference::Specific { provider_id } = &settings.provider_preference {
            req.defaults.provider_preference = Some(provider_id.clone());
        }
    }
    if req.defaults.tile_size.is_none() {
        req.defaults.tile_size = settings.tile_size_override;
    }
    if req.defaults.tile_overlap.is_none() {
        req.defaults.tile_overlap = settings.tile_overlap;
    }
    if req.defaults.blend_mode.is_none() {
        req.defaults.blend_mode = settings.blend_mode.clone();
    }
    if req
        .defaults
        .naming_template
        .as_deref()
        .is_none_or(|s| s.trim().is_empty())
    {
        req.defaults.naming_template = Some(settings.naming_template.clone());
    }

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

pub fn retry_job_impl(state: &AppState, job_id: &str) -> Result<JobSnapshot, ApiError> {
    let record = state
        .orchestrator
        .retry_job(job_id)
        .map_err(|e| map_orchestrator_error(&e))?;
    Ok(job_record_to_snapshot(record))
}

#[tauri::command]
pub fn retry_job(
    state: tauri::State<'_, AppState>,
    job_id: String,
) -> Result<JobSnapshot, ApiError> {
    retry_job_impl(&state, &job_id)
}

pub fn pause_queue_impl(state: &AppState) -> Result<QueueSnapshot, ApiError> {
    state.orchestrator.pause_queue();
    get_queue_impl(state)
}

#[tauri::command]
pub fn pause_queue(state: tauri::State<'_, AppState>) -> Result<QueueSnapshot, ApiError> {
    pause_queue_impl(&state)
}

pub fn resume_queue_impl(state: &AppState) -> Result<QueueSnapshot, ApiError> {
    state.orchestrator.resume_queue();
    get_queue_impl(state)
}

#[tauri::command]
pub fn resume_queue(state: tauri::State<'_, AppState>) -> Result<QueueSnapshot, ApiError> {
    resume_queue_impl(&state)
}

pub fn get_queue_impl(state: &AppState) -> Result<QueueSnapshot, ApiError> {
    let active = state
        .orchestrator
        .db
        .get_active_job_id()
        .map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to read active job from database: {e}"),
            details: None,
            retryable: false,
        })?;
    let queued = state
        .orchestrator
        .db
        .get_queued_job_ids()
        .map_err(|e| ApiError {
            code: ErrorCode::StorageFailure,
            message: format!("Failed to read queued jobs from database: {e}"),
            details: None,
            retryable: false,
        })?;
    Ok(QueueSnapshot {
        paused: state.orchestrator.is_paused(),
        active_job_id: active,
        queued_job_ids: queued,
        revision: format!("rev-{}", chrono::Utc::now().timestamp_millis()),
    })
}

#[tauri::command]
pub fn get_queue(state: tauri::State<'_, AppState>) -> Result<QueueSnapshot, ApiError> {
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

pub fn get_jobs_history_impl(
    state: &AppState,
    limit: Option<usize>,
    cursor: Option<String>,
) -> Result<JobHistoryPage, ApiError> {
    if let Some(0) = limit {
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: "Page limit must be at least 1".to_string(),
            details: None,
            retryable: false,
        });
    }

    let capped_limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).min(MAX_PAGE_SIZE);

    let (records, next_cursor) = state
        .orchestrator
        .db
        .list_jobs_page(capped_limit, cursor.as_deref())
        .map_err(|e| match e {
            DatabaseError::InvalidCursor(msg) => ApiError {
                code: ErrorCode::InvalidArgument,
                message: msg,
                details: None,
                retryable: false,
            },
            other => ApiError {
                code: ErrorCode::StorageFailure,
                message: other.to_string(),
                details: None,
                retryable: false,
            },
        })?;

    Ok(JobHistoryPage {
        jobs: records.into_iter().map(job_record_to_snapshot).collect(),
        next_cursor,
    })
}

#[tauri::command]
pub fn get_jobs_history(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
    cursor: Option<String>,
) -> Result<JobHistoryPage, ApiError> {
    get_jobs_history_impl(&state, limit, cursor)
}

#[tauri::command]
pub fn list_job_history(
    state: tauri::State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> Result<JobHistoryPage, ApiError> {
    get_jobs_history_impl(&state, limit, cursor)
}

pub const CURRENT_SETTINGS_SCHEMA_VERSION: u32 = 1;

pub fn preserve_invalid_settings_file(path: &Path, reason: &str) -> PathBuf {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let backup_path = path.with_extension(format!("json.{reason}.{timestamp}"));
    let _ = std::fs::copy(path, &backup_path);
    backup_path
}

fn migrate_settings_v0_to_v1(mut val: serde_json::Value) -> Result<AppSettings, ApiError> {
    let obj = match val.as_object_mut() {
        Some(o) => o,
        None => {
            return Err(ApiError {
                code: ErrorCode::InvalidArgument,
                message: "Settings JSON root must be an object".to_string(),
                details: None,
                retryable: false,
            });
        }
    };

    // Migrate legacy metadataPolicy: "strip" -> "stripAll"
    if let Some(mp) = obj
        .get("metadataPolicy")
        .or_else(|| obj.get("metadata_policy"))
    {
        if mp == "strip" {
            obj.insert(
                "metadataPolicy".to_string(),
                serde_json::Value::String("stripAll".to_string()),
            );
        }
    }

    // Ensure schemaVersion is set to 1
    obj.insert("schemaVersion".to_string(), serde_json::Value::from(1));

    // Convert snake_case legacy keys to camelCase if present
    let keys_to_convert = [
        ("output_directory", "outputDirectory"),
        ("models_directory", "modelsDirectory"),
        ("output_format", "outputFormat"),
        ("default_model_id", "defaultModelId"),
        ("default_model_variant_id", "defaultModelVariantId"),
        ("default_target_scale", "defaultTargetScale"),
        ("naming_template", "namingTemplate"),
        ("metadata_policy", "metadataPolicy"),
        ("preserve_gps", "preserveGps"),
        ("provider_preference", "providerPreference"),
        ("tile_size_override", "tileSizeOverride"),
        ("tile_overlap", "tileOverlap"),
        ("blend_mode", "blendMode"),
        ("gpu_device_id", "gpuDeviceId"),
        ("overwrite_existing", "overwriteExisting"),
        ("check_for_updates", "checkForUpdates"),
    ];

    for (snake, camel) in keys_to_convert {
        if let Some(v) = obj.remove(snake) {
            obj.entry(camel.to_string()).or_insert(v);
        }
    }

    // Merge into AppSettings defaults for any missing fields
    let default_val = serde_json::to_value(AppSettings::default()).map_err(|e| ApiError {
        code: ErrorCode::Internal,
        message: format!("Failed to serialize default settings: {e}"),
        details: None,
        retryable: false,
    })?;

    let mut merged = default_val.as_object().unwrap().clone();
    for (k, v) in obj.iter() {
        merged.insert(k.clone(), v.clone());
    }

    let merged_val = serde_json::Value::Object(merged);
    let settings: AppSettings = serde_json::from_value(merged_val).map_err(|e| ApiError {
        code: ErrorCode::InvalidArgument,
        message: format!("Failed to parse migrated settings v0->v1: {e}"),
        details: None,
        retryable: false,
    })?;

    Ok(settings)
}

pub fn load_or_migrate_settings(path: &Path) -> Result<AppSettings, ApiError> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = std::fs::read_to_string(path).map_err(|e| ApiError {
        code: match e.kind() {
            std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            _ => ErrorCode::StorageFailure,
        },
        message: format!("Failed to read settings file '{}': {}", path.display(), e),
        details: None,
        retryable: false,
    })?;

    let val: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            let backup_path = preserve_invalid_settings_file(path, "corrupt");
            return Err(ApiError {
                code: ErrorCode::StorageFailure,
                message: format!(
                    "Malformed settings JSON (original preserved at '{}'): {}",
                    backup_path.display(),
                    e
                ),
                details: None,
                retryable: false,
            });
        }
    };

    let schema_version = val
        .get("schemaVersion")
        .or_else(|| val.get("schema_version"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    if schema_version > CURRENT_SETTINGS_SCHEMA_VERSION {
        let backup_path = preserve_invalid_settings_file(path, "incompatible");
        return Err(ApiError {
            code: ErrorCode::InvalidArgument,
            message: format!(
                "Unsupported settings schema version {} (supported: {}, original preserved at '{}')",
                schema_version,
                CURRENT_SETTINGS_SCHEMA_VERSION,
                backup_path.display()
            ),
            details: None,
            retryable: false,
        });
    }

    if schema_version == 0 {
        let migrated = migrate_settings_v0_to_v1(val)?;
        validate_settings(&migrated)?;
        atomic_write_settings(path, &migrated)?;
        return Ok(migrated);
    }

    match serde_json::from_value::<AppSettings>(val) {
        Ok(settings) => {
            if let Err(e) = validate_settings(&settings) {
                let backup_path = preserve_invalid_settings_file(path, "invalid");
                return Err(ApiError {
                    code: ErrorCode::InvalidArgument,
                    message: format!(
                        "Invalid settings values (original preserved at '{}'): {}",
                        backup_path.display(),
                        e.message
                    ),
                    details: None,
                    retryable: false,
                });
            }
            Ok(settings)
        }
        Err(e) => {
            let backup_path = preserve_invalid_settings_file(path, "invalid");
            Err(ApiError {
                code: ErrorCode::InvalidArgument,
                message: format!(
                    "Failed to parse settings schema v1 (original preserved at '{}'): {}",
                    backup_path.display(),
                    e
                ),
                details: None,
                retryable: false,
            })
        }
    }
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
            code: match e.kind() {
                std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
                _ => ErrorCode::StorageFailure,
            },
            message: format!("Failed to create settings directory: {}", e),
            details: None,
            retryable: false,
        })?;
    }

    let tmp_path = path.with_extension(format!("json.tmp.{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp_path, json_str.as_bytes()).map_err(|e| ApiError {
        code: match e.kind() {
            std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            _ => ErrorCode::StorageFailure,
        },
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

pub fn load_settings_impl(state: &AppState) -> Result<AppSettings, ApiError> {
    let loaded = load_or_migrate_settings(&state.settings_path)?;
    let mut s = state.settings.lock().unwrap();
    *s = loaded.clone();
    Ok(loaded)
}

#[tauri::command]
pub fn load_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, ApiError> {
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

    // Validate and create models directory if specified
    if let Some(ref dir) = new_settings.models_directory {
        let new_path = expand_home_dir(dir);
        if !new_path.as_os_str().is_empty() {
            std::fs::create_dir_all(&new_path).map_err(|e| ApiError {
                code: match e.kind() {
                    std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
                    _ => ErrorCode::StorageFailure,
                },
                message: format!(
                    "Failed to create models directory '{}': {}",
                    new_path.display(),
                    e
                ),
                details: None,
                retryable: false,
            })?;
            let mut root = state.models_root.lock().unwrap();
            *root = new_path.clone();
            state.orchestrator.set_models_root(&new_path);
        }
    }

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

pub fn shutdown_application_impl(state: &AppState, timeout: Duration) -> bool {
    state.orchestrator.shutdown(timeout).unwrap_or(false)
}

#[tauri::command]
pub fn close_window(
    state: tauri::State<'_, AppState>,
    window: tauri::Window,
) -> Result<(), ApiError> {
    shutdown_application_impl(&state, Duration::from_secs(3));
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
        _ => "image/png",
    };

    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

use resvera_core::{atomic_save_image, OutputFormat, UpscaleJobRequest as CoreJobRequest};
use resvera_desktop::commands::*;
use resvera_desktop::ipc_types::*;
use resvera_desktop::worker::QueueWorker;
use resvera_engine_ort::OrtEngine;
use resvera_models::{compute_file_sha256, ModelInstaller};
use resvera_persistence::AppDatabase;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_ipc_types_serialization_rules() {
    let fmt = OutputFormat::Webp {
        lossless: true,
        quality: None,
    };
    let json = serde_json::to_string(&fmt).unwrap();
    assert_eq!(
        json,
        "{\"kind\":\"webp\",\"lossless\":true,\"quality\":null}"
    );

    let error = ApiError {
        code: ErrorCode::ModelNotFound,
        message: "Model not found".into(),
        details: None,
        retryable: false,
    };
    let err_json = serde_json::to_string(&error).unwrap();
    assert!(err_json.contains("\"code\":\"modelNotFound\""));
}

#[test]
fn test_ipc_commands_workflow() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");
    install_test_model(&models_root);
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );
    let settings_path = temp.path().join("settings.json");
    let state = AppState {
        orchestrator,
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
    };

    // 1. Get runtime status
    let status = get_runtime_status_impl(&state).unwrap();
    assert!(status.offline_ready);
    assert_eq!(status.engine.id, "ort");

    // 2. List models with verified installer state
    let models = list_models_impl(&models_root);
    assert_eq!(models.len(), 5);
    assert_eq!(models[0].id, "realesrgan-x4plus");
    assert!(models[0].installed); // Installed via verified test model fixture!
    assert!(!models[1].installed); // Not installed

    // 3. Settings load and save
    let default_settings = load_settings_impl(&state);
    assert_eq!(default_settings.schema_version, 1);

    let mut new_settings = default_settings.clone();
    new_settings.theme = "dark".into();
    let saved = save_settings_impl(&state, new_settings).unwrap();
    assert_eq!(saved.theme, "dark");
    assert!(state.settings_path.exists());

    // 4. Create and retrieve job
    let input_path = temp.path().join("photo.png");
    let img = image::RgbImage::new(16, 16);
    atomic_save_image(&img, &input_path, &OutputFormat::Png, None).unwrap();

    let req = CoreJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };

    let snapshot = create_upscale_job_impl(&state, req).unwrap();
    assert_eq!(snapshot.state, "queued");

    let fetched = get_job_impl(&state, &snapshot.id).unwrap();
    assert_eq!(fetched.id, snapshot.id);
    assert_eq!(fetched.state, "queued");

    // 5. Job history list
    let history = get_jobs_history_impl(&state, 10).unwrap();
    assert_eq!(history.jobs.len(), 1);
    assert_eq!(history.jobs[0].id, snapshot.id);
}

#[test]
fn test_settings_transactional_failure_does_not_mutate_in_memory() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        temp.path().join("models"),
    );

    // Target a path inside a read-only or non-creatable file to induce failure
    let invalid_dir = temp.path().join("not_a_directory");
    std::fs::write(&invalid_dir, b"file content").unwrap();
    let invalid_settings_path = invalid_dir.join("sub").join("settings.json");

    let initial = AppSettings::default();
    let state = AppState {
        orchestrator,
        settings: Arc::new(Mutex::new(initial.clone())),
        settings_path: invalid_settings_path,
    };

    let mut modified = initial.clone();
    modified.theme = "light".into();

    let result = save_settings_impl(&state, modified);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, ErrorCode::StorageFailure);

    // In-memory state remains untouched!
    assert_eq!(state.settings.lock().unwrap().theme, initial.theme);
}

#[test]
fn test_path_validation_and_rejection() {
    assert!(validate_path("").is_err());
    assert!(validate_path("   ").is_err());
    assert!(validate_path("path/with/\0null").is_err());
    assert!(validate_path("non_existent_file_xyz.png").is_err());

    assert!(validate_output_directory("path/with/\0null").is_err());
}

#[test]
fn test_settings_security_validation() {
    let invalid_settings = AppSettings {
        schema_version: 999,
        ..Default::default()
    };
    assert!(validate_settings(&invalid_settings).is_err());

    let null_out = AppSettings {
        output_directory: Some("/tmp/out\0side".into()),
        ..Default::default()
    };
    assert!(validate_settings(&null_out).is_err());

    let null_mod = AppSettings {
        models_directory: Some("/tmp/models\0bad".into()),
        ..Default::default()
    };
    assert!(validate_settings(&null_mod).is_err());

    let empty_template = AppSettings {
        naming_template: "".into(),
        ..Default::default()
    };
    assert!(validate_settings(&empty_template).is_err());

    let bad_metadata = AppSettings {
        metadata_policy: "exploitInjectedPolicy".into(),
        ..Default::default()
    };
    assert!(validate_settings(&bad_metadata).is_err());

    let bad_theme = AppSettings {
        theme: "<script>alert(1)</script>".into(),
        ..Default::default()
    };
    assert!(validate_settings(&bad_theme).is_err());
}

#[test]
fn test_background_queue_worker_execution() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");
    install_test_model(&models_root);
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );
    let settings_path = temp.path().join("settings.json");
    let state = AppState {
        orchestrator,
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
    };

    let input_path = temp.path().join("worker_photo.png");
    let img = image::RgbImage::new(16, 16);
    atomic_save_image(&img, &input_path, &OutputFormat::Png, None).unwrap();

    let req = CoreJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };

    let job = create_upscale_job_impl(&state, req).unwrap();
    assert_eq!(job.state, "queued");

    // Start worker
    let mut worker = QueueWorker::start(state.clone());

    // Wait for worker to pick up and process the job
    let mut processed = false;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(50));
        let current = get_job_impl(&state, &job.id).unwrap();
        // Since dummy model bytes are not valid ONNX graphs, the worker truthfully
        // fails closed rather than faking success.
        if current.state == "failed" {
            assert!(current.error.is_some());
            assert_eq!(current.progress.unwrap().fraction, 0.0);
            processed = true;
            break;
        }
    }
    worker.stop();
    assert!(
        processed,
        "Background worker should have picked up and executed the queued job"
    );
}

fn install_test_model(models_root: &std::path::Path) {
    let stage = models_root.join(".test-stage");
    let artifacts = stage.join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let artifact = artifacts.join("model.onnx");
    std::fs::write(&artifact, b"ipc-contract-model").unwrap();
    let manifest = serde_json::json!({
        "schema_version": 1,
        "id": "realesrgan-x4plus",
        "package_version": "1.0.0",
        "display_name": "IPC test model",
        "family": "rrdb",
        "category": "test",
        "description": "Test-only package",
        "license": {
            "spdx": "MIT",
            "upstream_url": "https://example.invalid/model",
            "redistribution_review": "test-only"
        },
        "provenance": {
            "upstream_repository": "https://example.invalid/model",
            "upstream_revision": "test",
            "source_weight_name": "test.pth",
            "source_weight_sha256": "0".repeat(64),
            "export_recipe": "test"
        },
        "variants": [{
            "id": "default",
            "native_scale": 4,
            "strength": null,
            "artifact": "artifacts/model.onnx"
        }],
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
            "recommended": 32,
            "overlap": 8,
            "window_size": null,
            "static_shapes_required": false
        },
        "compatibility": {
            "engine": "onnx-runtime",
            "minimum_engine_version": "1.28.0",
            "validated_providers": ["cpu"],
            "validated_precisions": ["fp32"]
        },
        "artifacts": [{
            "path": "artifacts/model.onnx",
            "size_bytes": std::fs::metadata(&artifact).unwrap().len(),
            "sha256": compute_file_sha256(&artifact).unwrap()
        }]
    });
    std::fs::write(
        stage.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    ModelInstaller::new(models_root)
        .install_package(&stage)
        .unwrap();
}

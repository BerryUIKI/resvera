use resvera_core::{
    atomic_save_image, JobOrchestrator, OutputFormat, UpscaleJobRequest as CoreJobRequest,
};
use resvera_desktop::commands::*;
use resvera_desktop::ipc_types::*;
use resvera_desktop::preview_scope::PreviewScope;
use resvera_desktop::worker::QueueWorker;
use resvera_engine_ort::OrtEngine;
use resvera_models::{compute_file_sha256, ModelInstaller};
use resvera_persistence::{AppDatabase, JobRecord};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
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

    let req_json = r#"{
        "inputPath": "/path/to/img.png",
        "outputDirectory": "/out",
        "modelId": "realesrgan-x4plus",
        "modelVariantId": "default",
        "targetScale": 4,
        "outputFormat": {"kind": "png"},
        "overwrite": false,
        "tileSize": 256,
        "providerPreference": "cpu"
    }"#;
    let parsed_req: CoreJobRequest = serde_json::from_str(req_json).unwrap();
    assert_eq!(parsed_req.input_path, "/path/to/img.png");
    assert_eq!(parsed_req.model_variant_id, "default");
    assert_eq!(parsed_req.tile_size, Some(256));
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
        models_root: Arc::new(Mutex::new(models_root.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
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
    let default_settings = load_settings_impl(&state).unwrap();
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
        tile_overlap: Some(16),
        blend_mode: Some("cosine".to_string()),
        naming_template: Some("{stem}_4x_custom".to_string()),
        provider_preference: Some("cpu".to_string()),
    };

    let snapshot = create_upscale_job_impl(&state, req).unwrap();
    assert_eq!(snapshot.state, "queued");
    assert_eq!(snapshot.tile_size, Some(32));
    assert_eq!(snapshot.tile_overlap, Some(16));
    assert_eq!(snapshot.blend_mode.as_deref(), Some("cosine"));
    assert_eq!(
        snapshot.naming_template.as_deref(),
        Some("{stem}_4x_custom")
    );

    let fetched = get_job_impl(&state, &snapshot.id).unwrap();
    assert_eq!(fetched.tile_overlap, Some(16));
    assert_eq!(fetched.blend_mode.as_deref(), Some("cosine"));
    assert_eq!(fetched.naming_template.as_deref(), Some("{stem}_4x_custom"));
    assert_eq!(fetched.id, snapshot.id);
    assert_eq!(fetched.state, "queued");

    // 5. Job history list
    let history = get_jobs_history_impl(&state, Some(10), None).unwrap();
    assert_eq!(history.jobs.len(), 1);
    assert_eq!(history.jobs[0].id, snapshot.id);
    assert_eq!(history.next_cursor, None);
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
        models_root: Arc::new(Mutex::new(temp.path().join("models"))),
        settings: Arc::new(Mutex::new(initial.clone())),
        settings_path: invalid_settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
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
    // 1. Valid settings with all allowed metadata policies and themes
    for policy in &["preserveSafe", "stripAll", "preserveAll"] {
        let valid = AppSettings {
            metadata_policy: (*policy).into(),
            ..Default::default()
        };
        assert!(validate_settings(&valid).is_ok());
    }

    for theme in &["dark", "light", "system"] {
        let valid = AppSettings {
            theme: (*theme).into(),
            ..Default::default()
        };
        assert!(validate_settings(&valid).is_ok());
    }

    // 2. Schema version validation
    let invalid_settings = AppSettings {
        schema_version: 999,
        ..Default::default()
    };
    assert!(validate_settings(&invalid_settings).is_err());

    // 3. Null bytes and empty paths
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

    // 4. Invalid metadata policies (including legacy "strip" which must fail in favor of "stripAll")
    let legacy_strip = AppSettings {
        metadata_policy: "strip".into(),
        ..Default::default()
    };
    assert_eq!(
        validate_settings(&legacy_strip).unwrap_err().code,
        ErrorCode::InvalidArgument
    );

    let bad_metadata = AppSettings {
        metadata_policy: "exploitInjectedPolicy".into(),
        ..Default::default()
    };
    assert!(validate_settings(&bad_metadata).is_err());

    // 5. Invalid themes
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
        models_root: Arc::new(Mutex::new(models_root.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
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
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
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
    // Verify that worker continues processing sequential queued jobs without frontend process_next_job calls
    let req2 = CoreJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: true,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: Some("cpu".to_string()),
    };
    let job2 = create_upscale_job_impl(&state, req2).unwrap();
    assert_eq!(job2.state, "queued");

    let mut processed2 = false;
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(50));
        let current2 = get_job_impl(&state, &job2.id).unwrap();
        if current2.state == "failed" {
            processed2 = true;
            break;
        }
    }

    worker.stop();
    assert!(
        processed,
        "Background worker should have picked up and executed the first queued job"
    );
    assert!(
        processed2,
        "Background worker should have automatically picked up and executed the second queued job without client process_next_job"
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

#[test]
fn test_uninstall_model_success_and_validation() {
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
        models_root: Arc::new(Mutex::new(models_root.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Verify model is initially installed
    let models = list_models_impl(&state.models_root.lock().unwrap());
    assert!(models
        .iter()
        .any(|m| m.id == "realesrgan-x4plus" && m.installed));

    // Invalid model IDs should be rejected
    assert_eq!(
        uninstall_model_impl(&state, "".into()).unwrap_err().code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        uninstall_model_impl(&state, "model\0bad".into())
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        uninstall_model_impl(&state, "../traversal".into())
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        uninstall_model_impl(&state, "nested/path".into())
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );

    // Attempting to uninstall when an active/queued job references the model fails with ModelInUse
    let job = resvera_persistence::JobRecord {
        id: "active-job-1".into(),
        state: "queued".into(),
        input_path: "/dummy.png".into(),
        output_path: None,
        preview_path: None,
        model_id: "realesrgan-x4plus".into(),
        model_package_version: "1.0.0".into(),
        model_variant_id: "default".into(),
        target_scale: 4,
        engine_id: "ort".into(),
        provider_id: None,
        progress_fraction: 0.0,
        progress_stage: "queued".into(),
        error_code: None,
        error_message: None,
        output_directory: None,
        output_format_json: None,
        overwrite: false,
        tile_size: None,
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        created_at: "2026-09-20T00:00:00Z".into(),
        updated_at: "2026-09-20T00:00:00Z".into(),
    };
    state.orchestrator.db.insert_job(&job).unwrap();

    let in_use_err = uninstall_model_impl(&state, "realesrgan-x4plus".into()).unwrap_err();
    assert_eq!(in_use_err.code, ErrorCode::ModelInUse);
    assert!(in_use_err
        .message
        .contains("referenced by 1 active or queued job(s)"));

    // Once the job transitions to terminal state, uninstall succeeds
    state.orchestrator.db.cancel_job("active-job-1").unwrap();

    // Uninstall installed model successfully
    let uninstalled = uninstall_model_impl(&state, "realesrgan-x4plus".into()).unwrap();
    assert!(uninstalled);

    // After uninstallation, list_models_impl reflects installed: false
    let models_after = list_models_impl(&state.models_root.lock().unwrap());
    assert!(models_after
        .iter()
        .any(|m| m.id == "realesrgan-x4plus" && !m.installed));

    // Idempotent: uninstalling already removed model returns false without error
    let uninstalled_again = uninstall_model_impl(&state, "realesrgan-x4plus".into()).unwrap();
    assert!(!uninstalled_again);
}

#[tokio::test]
async fn test_install_model_success_and_validation() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");

    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );
    let settings_path = temp.path().join("settings.json");
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Initially not installed
    let models_before = list_models_impl(&state.models_root.lock().unwrap());
    assert!(models_before
        .iter()
        .any(|m| m.id == "realesrgan-x4plus" && !m.installed));

    // Invalid model ID rejected
    assert_eq!(
        install_model_impl(&state, "".into())
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        install_model_impl(&state, "invalid/id".into())
            .await
            .unwrap_err()
            .code,
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        install_model_impl(&state, "nonexistent-model".into())
            .await
            .unwrap_err()
            .code,
        ErrorCode::ModelNotFound
    );

    let has_fixture = [
        PathBuf::from("artifacts/exports/realesrgan-x4plus/model.onnx"),
        PathBuf::from("../artifacts/exports/realesrgan-x4plus/model.onnx"),
        PathBuf::from("../../artifacts/exports/realesrgan-x4plus/model.onnx"),
    ]
    .iter()
    .any(|p| p.is_file());

    if has_fixture {
        // Install model
        let summary = install_model_impl(&state, "realesrgan-x4plus".into())
            .await
            .unwrap();
        assert!(summary.installed);
        assert_eq!(summary.id, "realesrgan-x4plus");

        // Check list_models_impl now reports installed: true
        let models_after = list_models_impl(&state.models_root.lock().unwrap());
        assert!(models_after
            .iter()
            .any(|m| m.id == "realesrgan-x4plus" && m.installed));
    }
}

#[test]
fn test_save_settings_dynamic_models_root() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let root_a = temp.path().join("models_a");
    let root_b = temp.path().join("models_b");
    install_test_model(&root_a);

    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &root_a,
    );
    let settings_path = temp.path().join("settings.json");
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(root_a.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Initially uses root_a where model is installed
    let models = list_models_impl(&state.models_root.lock().unwrap());
    assert!(models
        .iter()
        .any(|m| m.id == "realesrgan-x4plus" && m.installed));

    // Switch settings to root_b
    let updated_settings = AppSettings {
        models_directory: Some(root_b.to_str().unwrap().to_string()),
        ..Default::default()
    };
    save_settings_impl(&state, updated_settings).unwrap();

    // models_root in AppState and orchestrator should now both be updated to root_b
    assert_eq!(*state.models_root.lock().unwrap(), root_b);
    assert_eq!(state.orchestrator.models_root(), root_b);

    // list_models_impl against updated models_root now reports installed: false
    let models_switched = list_models_impl(&state.models_root.lock().unwrap());
    assert!(models_switched
        .iter()
        .any(|m| m.id == "realesrgan-x4plus" && !m.installed));
}

#[test]
fn test_stage_input_image_validation_and_staging() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );
    let staging_dir = temp.path().join("staging");
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root)),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: staging_dir.clone(),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // 1. Rejects empty data
    let empty_err = stage_input_image_impl(&state, "photo.png".into(), vec![]).unwrap_err();
    assert_eq!(empty_err.code, ErrorCode::InvalidArgument);

    // 2. Rejects unsupported extensions and formats
    let ext_err = stage_input_image_impl(&state, "malware.exe".into(), vec![1, 2, 3]).unwrap_err();
    assert_eq!(ext_err.code, ErrorCode::UnsupportedFormat);

    let bmp_err =
        stage_input_image_impl(&state, "test.bmp".into(), vec![0x42, 0x4D, 0, 0]).unwrap_err();
    assert_eq!(bmp_err.code, ErrorCode::UnsupportedFormat);

    let fake_png_err =
        stage_input_image_impl(&state, "fake.png".into(), vec![1, 2, 3, 4]).unwrap_err();
    assert_eq!(fake_png_err.code, ErrorCode::UnsupportedFormat);

    // 3. Successfully stages image and sanitizes traversal in file_name
    let sample_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG header
    let staged_path_str = stage_input_image_impl(
        &state,
        "../../suspicious/path/my_photo.PNG".into(),
        sample_bytes.clone(),
    )
    .unwrap();

    let staged_path = PathBuf::from(&staged_path_str);
    assert!(staged_path.exists());
    let canonical_staging =
        resvera_core::strip_verbatim_prefix(staging_dir.canonicalize().unwrap_or(staging_dir));
    assert!(staged_path.starts_with(&canonical_staging));
    assert!(staged_path_str.ends_with("my_photo.PNG"));

    // Verify content on disk
    let read_back = std::fs::read(&staged_path).unwrap();
    assert_eq!(read_back, sample_bytes);
}

#[test]
fn test_retry_job_ipc_workflow_and_active_state_rejection() {
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
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root)),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    let input_path = temp.path().join("sample.png");
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
        tile_size: Some(128),
        tile_overlap: Some(24),
        blend_mode: Some("linear".to_string()),
        naming_template: Some("{stem}_retry_test".to_string()),
        provider_preference: Some("cpu".to_string()),
    };

    // 1. Submit initial job
    let snap1 = create_upscale_job_impl(&state, req.clone()).unwrap();
    assert_eq!(snap1.state, "queued");
    assert_eq!(snap1.tile_size, Some(128));
    assert_eq!(snap1.tile_overlap, Some(24));
    assert_eq!(snap1.blend_mode.as_deref(), Some("linear"));
    assert_eq!(snap1.naming_template.as_deref(), Some("{stem}_retry_test"));

    // 2. Cannot submit duplicate active job for same input
    let dup_err = create_upscale_job_impl(&state, req.clone()).unwrap_err();
    assert_eq!(dup_err.code, ErrorCode::InvalidArgument);

    // 3. Cannot retry an active job
    let retry_active_err = retry_job_impl(&state, &snap1.id).unwrap_err();
    assert_eq!(retry_active_err.code, ErrorCode::InvalidArgument);

    // 4. Process job to completion (dummy ONNX model truthfully fails execution)
    let completed = state.orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(completed.state, "failed");

    // 5. Retry failed job via IPC command
    let retried = retry_job_impl(&state, &snap1.id).unwrap();
    assert_ne!(retried.id, snap1.id);
    assert_eq!(retried.state, "queued");
    assert_eq!(retried.model_id, "realesrgan-x4plus");
    assert_eq!(retried.target_scale, 4);
    assert_eq!(retried.input_path, snap1.input_path);
    assert_eq!(retried.tile_size, Some(128));
    assert_eq!(retried.tile_overlap, Some(24));
    assert_eq!(retried.blend_mode.as_deref(), Some("linear"));
    assert_eq!(
        retried.naming_template.as_deref(),
        Some("{stem}_retry_test")
    );

    // 6. Cancel the retried job
    let cancelled = cancel_job_impl(&state, &retried.id).unwrap();
    assert_eq!(cancelled.state, "cancelled");
}

#[test]
fn test_coordinated_application_shutdown() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_test_model(&models_root);

    let engine = Arc::new(OrtEngine::new());
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root)),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    let in_path = temp.path().join("shutdown_input.png");
    let img = image::RgbImage::new(32, 32);
    atomic_save_image(&img, &in_path, &OutputFormat::Png, None).unwrap();

    let req = CoreJobRequest {
        input_path: in_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: Some("cpu".to_string()),
    };

    let job = create_upscale_job_impl(&state, req).unwrap();
    assert_eq!(job.state, "queued");

    // Start background worker
    let mut worker = QueueWorker::start(state.clone());

    // Coordinate application shutdown with bounded timeout
    let clean = worker.stop_with_timeout(Duration::from_millis(500));
    assert!(clean);

    // Queue must be paused and worker stopped
    assert!(state.orchestrator.is_paused());

    // Job in DB must be in terminal or recoverable state (queued, cancelled, or interrupted)
    let retrieved = get_job_impl(&state, &job.id).unwrap();
    assert!(
        ["queued", "cancelled", "interrupted", "failed"].contains(&retrieved.state.as_str()),
        "Job must be in a clean persisted state after shutdown, got {}",
        retrieved.state
    );
}

#[test]
fn test_job_history_bounded_cursor_pagination_workflow() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_test_model(&models_root);

    let engine = Arc::new(OrtEngine::new());
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root)),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Insert 5 completed jobs with deterministic timestamps into DB
    for i in 1..=5 {
        let job = JobRecord {
            id: format!("hist-job-{i}"),
            state: "completed".to_string(),
            input_path: "/dummy/input.png".to_string(),
            output_path: Some("/dummy/out.png".to_string()),
            preview_path: None,
            model_id: "realesrgan-x4plus".to_string(),
            model_package_version: "1.0.0".to_string(),
            model_variant_id: "default".to_string(),
            target_scale: 4,
            engine_id: "ort".to_string(),
            provider_id: Some("cpu".to_string()),
            progress_fraction: 1.0,
            progress_stage: "completed".to_string(),
            error_code: None,
            error_message: None,
            output_directory: None,
            output_format_json: None,
            overwrite: false,
            tile_size: None,
            tile_overlap: None,
            blend_mode: None,
            naming_template: None,
            created_at: format!("2026-09-21T10:0{i}:00Z"),
            updated_at: format!("2026-09-21T10:0{i}:00Z"),
        };
        db.insert_job(&job).unwrap();
    }

    // Page 1: limit 2
    let page1 = get_jobs_history_impl(&state, Some(2), None).unwrap();
    assert_eq!(page1.jobs.len(), 2);
    assert_eq!(page1.jobs[0].id, "hist-job-5");
    assert_eq!(page1.jobs[1].id, "hist-job-4");
    assert!(page1.next_cursor.is_some());

    // Page 2: limit 2 with cursor from page 1
    let page2 = get_jobs_history_impl(&state, Some(2), page1.next_cursor).unwrap();
    assert_eq!(page2.jobs.len(), 2);
    assert_eq!(page2.jobs[0].id, "hist-job-3");
    assert_eq!(page2.jobs[1].id, "hist-job-2");
    assert!(page2.next_cursor.is_some());

    // Page 3: limit 2 with cursor from page 2
    let page3 = get_jobs_history_impl(&state, Some(2), page2.next_cursor).unwrap();
    assert_eq!(page3.jobs.len(), 1);
    assert_eq!(page3.jobs[0].id, "hist-job-1");
    assert_eq!(
        page3.next_cursor, None,
        "Final page must have next_cursor: None"
    );
}

#[test]
fn test_job_history_validation_and_bounds() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_test_model(&models_root);

    let engine = Arc::new(OrtEngine::new());
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root)),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // 1. Limit = 0 must be rejected with validation error
    let zero_res = get_jobs_history_impl(&state, Some(0), None);
    assert!(zero_res.is_err());
    let err = zero_res.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);

    // 2. Limit > MAX_PAGE_SIZE (100) must be capped at 100 without error
    let capped_res = get_jobs_history_impl(&state, Some(500), None);
    assert!(capped_res.is_ok());

    // 3. Invalid cursor format must return validation error
    let invalid_cursor_res =
        get_jobs_history_impl(&state, Some(10), Some("invalid-token!#%".to_string()));
    assert!(invalid_cursor_res.is_err());
    let err = invalid_cursor_res.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
}

#[test]
fn test_streaming_staging_upload_lifecycle() {
    use base64::Engine;
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");
    let staging_dir = temp.path().join("staging");
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    )
    .with_staging_dir(&staging_dir);

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root)),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: staging_dir.clone(),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // 1. Start upload session
    let session_id = start_staging_upload_impl(&state, "stream_test.png".to_string()).unwrap();

    // 2. Append chunks (Valid PNG header: 8 bytes, followed by chunk data)
    let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let chunk1_b64 = base64::engine::general_purpose::STANDARD.encode(&png_header[0..4]);
    let chunk2_b64 = base64::engine::general_purpose::STANDARD.encode(&png_header[4..8]);

    append_staging_chunk_impl(&state, &session_id, &chunk1_b64).unwrap();
    append_staging_chunk_impl(&state, &session_id, &chunk2_b64).unwrap();

    // 3. Finish upload session
    let staged_path_str = finish_staging_upload_impl(&state, &session_id).unwrap();
    let staged_path = PathBuf::from(&staged_path_str);
    assert!(staged_path.exists());
    assert!(staged_path.is_file());

    // Verify session was cleaned up
    assert!(state.staging_sessions.lock().unwrap().is_empty());

    // 4. Abort upload test (creates temp file and verifies deletion on abort)
    let abort_session = start_staging_upload_impl(&state, "abort_test.png".to_string()).unwrap();
    append_staging_chunk_impl(&state, &abort_session, &chunk1_b64).unwrap();
    let temp_file = staging_dir.join(format!(".upload_{abort_session}.tmp"));
    assert!(temp_file.exists());

    abort_staging_upload_impl(&state, &abort_session).unwrap();
    assert!(!temp_file.exists(), "Aborted staging file must be deleted");

    // 5. Single-shot base64 staging
    let full_b64 = base64::engine::general_purpose::STANDARD.encode(&png_header);
    let b64_staged =
        stage_input_image_base64_impl(&state, "b64_test.png".to_string(), full_b64).unwrap();
    assert!(PathBuf::from(&b64_staged).exists());
}

#[test]
fn test_staging_disk_leak_prevention_on_job_completion() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_test_model(&models_root);

    let staging_dir = temp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();

    let engine = Arc::new(OrtEngine::new());
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    )
    .with_staging_dir(&staging_dir);

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root)),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: staging_dir.clone(),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Stage an image
    let img = image::RgbImage::new(32, 32);
    let mut raw_png = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut raw_png);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();

    let staged_path_str =
        stage_input_image_impl(&state, "photo.png".into(), raw_png.clone()).unwrap();
    let staged_path = PathBuf::from(&staged_path_str);
    assert!(staged_path.exists());

    // Submit job referencing staged input
    let req = CoreJobRequest {
        input_path: staged_path_str.clone(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: Some("cpu".to_string()),
    };

    let job = create_upscale_job_impl(&state, req.clone()).unwrap();
    assert_eq!(job.state, "queued");
    assert!(staged_path.exists(), "Staged path must exist while queued");

    // Execute job
    let completed = state.orchestrator.process_next_job().unwrap().unwrap();
    // With dummy model weights, execution fails closed truthfully
    assert_eq!(completed.state, "failed");

    // Verify zero disk leak: staged input file must be removed upon terminal state!
    assert!(
        !staged_path.exists(),
        "Staged image file must be deleted after job completion to prevent disk leaks"
    );

    // 2. Cancellation test: cancel a queued job referencing staged input
    let staged_path_str2 = stage_input_image_impl(&state, "photo2.png".into(), raw_png).unwrap();
    let staged_path2 = PathBuf::from(&staged_path_str2);
    assert!(staged_path2.exists());

    let mut req2 = req;
    req2.input_path = staged_path_str2;
    let job2 = create_upscale_job_impl(&state, req2).unwrap();
    assert_eq!(job2.state, "queued");

    cancel_job_impl(&state, &job2.id).unwrap();
    assert!(
        !staged_path2.exists(),
        "Staged image file must be deleted on job cancellation"
    );
}

#[test]
fn test_database_failure_returns_typed_storage_failure_ipc_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("corrupted_queue.db");
    let db = AppDatabase::open(&db_path).unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        temp.path().join("models"),
    );

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(temp.path().join("models"))),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Verify healthy queue returns Ok
    let queue = get_queue_impl(&state).unwrap();
    assert_eq!(queue.queued_job_ids.len(), 0);

    // Drop table to induce a database failure
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch("DROP TABLE jobs;").unwrap();

    // get_queue_impl, pause_queue_impl, resume_queue_impl must return typed StorageFailure
    let err_get = get_queue_impl(&state).unwrap_err();
    assert_eq!(err_get.code, ErrorCode::StorageFailure);
    assert!(err_get.message.contains("database"));

    let err_pause = pause_queue_impl(&state).unwrap_err();
    assert_eq!(err_pause.code, ErrorCode::StorageFailure);

    let err_resume = resume_queue_impl(&state).unwrap_err();
    assert_eq!(err_resume.code, ErrorCode::StorageFailure);
}

#[test]
fn test_load_settings_malformed_json_preserves_corrupt_file_and_errors() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        temp.path().join("models"),
    );

    let settings_path = temp.path().join("settings.json");
    let corrupt_content = "{\"schemaVersion\": 1, \"theme\": \"dark\", MALFORMED_SYNTAX...";
    std::fs::write(&settings_path, corrupt_content).unwrap();

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(temp.path().join("models"))),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: settings_path.clone(),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    let result = load_settings_impl(&state);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::StorageFailure);
    assert!(err.message.contains("Malformed settings JSON"));

    // Check that diagnostic backup file was created
    let mut backup_found = false;
    for entry in std::fs::read_dir(temp.path()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if name.starts_with("settings.json.corrupt.") {
            backup_found = true;
            let preserved_bytes = std::fs::read_to_string(&path).unwrap();
            assert_eq!(preserved_bytes, corrupt_content);
        }
    }
    assert!(
        backup_found,
        "Corrupt settings file must be preserved for diagnosis"
    );
}

#[test]
fn test_load_settings_incompatible_schema_version_preserves_file_and_errors() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        temp.path().join("models"),
    );

    let settings_path = temp.path().join("settings.json");
    let future_content = "{\"schemaVersion\": 999, \"theme\": \"dark\", \"futureOption\": true}";
    std::fs::write(&settings_path, future_content).unwrap();

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(temp.path().join("models"))),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: settings_path.clone(),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    let result = load_settings_impl(&state);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
    assert!(err
        .message
        .contains("Unsupported settings schema version 999"));

    // Ensure the original file was NOT overwritten
    let current_disk = std::fs::read_to_string(&settings_path).unwrap();
    assert_eq!(current_disk, future_content);

    // Check that diagnostic backup was created
    let mut backup_found = false;
    for entry in std::fs::read_dir(temp.path()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if name.starts_with("settings.json.incompatible.") {
            backup_found = true;
            let preserved_bytes = std::fs::read_to_string(&path).unwrap();
            assert_eq!(preserved_bytes, future_content);
        }
    }
    assert!(backup_found, "Incompatible settings file must be preserved");
}

#[test]
fn test_load_settings_v0_migration_success() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        temp.path().join("models"),
    );

    let settings_path = temp.path().join("settings.json");
    // Legacy v0 settings: missing schemaVersion, snake_case key, legacy metadataPolicy 'strip'
    let v0_content = r#"{
        "output_directory": "/custom/export/dir",
        "metadata_policy": "strip",
        "theme": "light",
        "locale": "en-US"
    }"#;
    std::fs::write(&settings_path, v0_content).unwrap();

    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(temp.path().join("models"))),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: settings_path.clone(),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    let loaded = load_settings_impl(&state).unwrap();
    assert_eq!(loaded.schema_version, 1);
    assert_eq!(
        loaded.output_directory.as_deref(),
        Some("/custom/export/dir")
    );
    assert_eq!(loaded.metadata_policy, "stripAll"); // Migrated from 'strip'!
    assert_eq!(loaded.theme, "light");
    assert_eq!(loaded.locale, "en-US");

    // Verify migrated settings file committed to disk has schema_version 1
    let disk_content = std::fs::read_to_string(&settings_path).unwrap();
    let re_read: AppSettings = serde_json::from_str(&disk_content).unwrap();
    assert_eq!(re_read.schema_version, 1);
    assert_eq!(re_read.metadata_policy, "stripAll");
}

#[test]
fn test_save_settings_fails_on_uncreatable_models_dir() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        temp.path().join("models"),
    );

    // Create a regular file blocking directory creation
    let blocker_file = temp.path().join("blocker_file");
    std::fs::write(&blocker_file, b"content").unwrap();
    let uncreatable_dir = blocker_file.join("sub_dir_models");

    let initial = AppSettings::default();
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(temp.path().join("models"))),
        settings: Arc::new(Mutex::new(initial.clone())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    let mut invalid_settings = initial.clone();
    invalid_settings.models_directory = Some(uncreatable_dir.to_str().unwrap().to_string());

    let result = save_settings_impl(&state, invalid_settings);
    assert!(
        result.is_err(),
        "Saving settings with uncreatable models dir must fail"
    );
    let err = result.unwrap_err();
    assert!(
        err.code == ErrorCode::StorageFailure || err.code == ErrorCode::PermissionDenied,
        "Expected StorageFailure or PermissionDenied, got {:?}",
        err.code
    );

    // In-memory settings must remain unchanged
    assert_eq!(
        state.settings.lock().unwrap().models_directory,
        initial.models_directory
    );
}

#[tokio::test]
async fn test_install_model_e2e_and_execution() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");

    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );
    let settings_path = temp.path().join("settings.json");
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    let has_fixture = [
        PathBuf::from("artifacts/exports/realesrgan-x4plus/model.onnx"),
        PathBuf::from("../artifacts/exports/realesrgan-x4plus/model.onnx"),
        PathBuf::from("../../artifacts/exports/realesrgan-x4plus/model.onnx"),
    ]
    .iter()
    .any(|p| p.is_file());

    if has_fixture {
        // 1. Clean installation of model into empty directory
        let summary = install_model_impl(&state, "realesrgan-x4plus".into())
            .await
            .expect("Model installation should succeed");
        assert!(summary.installed);
        assert_eq!(summary.id, "realesrgan-x4plus");

        // 2. Verify current.json and structure
        let current_json = models_root.join("realesrgan-x4plus").join("current.json");
        assert!(current_json.is_file(), "current.json must be present");
        let current_content = std::fs::read_to_string(&current_json).unwrap();
        assert!(current_content.contains("\"active_version\""));

        // 3. Verify orchestrator can run upscale with this newly installed model
        let input_image = temp.path().join("input.png");
        let img = image::RgbImage::new(32, 32);
        img.save(&input_image).unwrap();

        let job = create_upscale_job_impl(
            &state,
            CoreJobRequest {
                input_path: input_image.to_str().unwrap().to_string(),
                output_directory: temp.path().join("output").to_str().unwrap().to_string(),
                model_id: "realesrgan-x4plus".into(),
                model_variant_id: "default".into(),
                target_scale: 4,
                output_format: OutputFormat::Png,
                overwrite: true,
                tile_size: Some(32),
                tile_overlap: Some(16),
                blend_mode: None,
                naming_template: None,
                provider_preference: None,
            },
        )
        .unwrap();

        assert_eq!(job.state, "queued");

        // Process the job
        let finished_job = state.orchestrator.process_next_job().unwrap().unwrap();
        assert_eq!(finished_job.state, "succeeded");
        assert!(finished_job.output_path.is_some());
        assert!(std::path::Path::new(finished_job.output_path.as_ref().unwrap()).is_file());
    }
}

#[tokio::test]
async fn test_cancel_model_install_and_sweep() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");

    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );
    let settings_path = temp.path().join("settings.json");
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Test cancel_model_install on non-active returns false
    assert!(!cancel_model_install_impl(&state, "nonexistent".into()).unwrap());

    // Test get_model_install_progress
    assert!(get_model_install_progress_impl(&state, "nonexistent".into()).is_none());
}

#[tokio::test]
async fn test_import_model_file_flow() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(OrtEngine::with_provider("cpu"));
    let models_root = temp.path().join("models");

    let orchestrator = resvera_core::JobOrchestrator::with_models_root(
        db,
        engine,
        temp.path().join("previews"),
        &models_root,
    );
    let settings_path = temp.path().join("settings.json");
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(models_root.clone())),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path,
        staging_dir: temp.path().join("staging"),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope: Arc::new(PreviewScope::default()),
    };

    // Create a dummy file with wrong hash to verify hash rejection
    let fake_file = temp.path().join("fake_realesrgan.onnx");
    std::fs::write(&fake_file, b"corrupted-or-fake-model-weights").unwrap();

    let err = import_model_file_impl(
        &state,
        fake_file.to_str().unwrap().to_string(),
        Some("realesrgan-x4plus".into()),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::HashMismatch);

    // Now import genuine file if available in project
    let genuine_path = [
        PathBuf::from("artifacts/exports/realesrgan-x4plus/model.onnx"),
        PathBuf::from("../artifacts/exports/realesrgan-x4plus/model.onnx"),
        PathBuf::from("../../artifacts/exports/realesrgan-x4plus/model.onnx"),
    ]
    .into_iter()
    .find(|p| p.is_file());

    if let Some(genuine_path) = genuine_path {
        let summary = import_model_file_impl(
            &state,
            genuine_path.to_str().unwrap().to_string(),
            Some("realesrgan-x4plus".into()),
        )
        .unwrap();
        assert!(summary.installed);

        // Uninstall works
        let uninstalled = uninstall_model_impl(&state, "realesrgan-x4plus".into()).unwrap();
        assert!(uninstalled);
    }
}

#[test]
fn test_read_image_data_security_scoping() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let engine = Arc::new(OrtEngine::new());
    let preview_dir = temp.path().join("previews");
    std::fs::create_dir_all(&preview_dir).unwrap();
    let staging_dir = temp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();
    let orchestrator = JobOrchestrator::with_models_root(
        db,
        engine,
        preview_dir.clone(),
        temp.path().join("models"),
    )
    .with_staging_dir(&staging_dir);

    let preview_scope = Arc::new(PreviewScope::new());
    let state = AppState {
        orchestrator,
        models_root: Arc::new(Mutex::new(temp.path().join("models"))),
        settings: Arc::new(Mutex::new(AppSettings::default())),
        settings_path: temp.path().join("settings.json"),
        staging_dir: staging_dir.clone(),
        staging_sessions: Arc::new(Mutex::new(HashMap::new())),
        active_installs: Arc::new(Mutex::new(HashMap::new())),
        install_progress: Arc::new(Mutex::new(HashMap::new())),
        preview_scope,
    };

    // 1. Rejects empty path and null bytes
    let err = read_image_data_impl(&state, "".into()).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);

    let err = read_image_data_impl(&state, "foo\0bar.png".into()).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);

    // 2. Rejects relative paths
    let err = read_image_data_impl(&state, "relative/path.png".into()).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);

    // 3. Rejects disallowed extensions (even if the file exists)
    let secret_txt = temp.path().join("secret.txt");
    std::fs::write(&secret_txt, b"super-secret-passwords").unwrap();
    let err = read_image_data_impl(&state, secret_txt.to_str().unwrap().into()).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);

    // 4. Rejects non-existent file
    let missing = staging_dir.join("missing.png");
    let err = read_image_data_impl(&state, missing.to_str().unwrap().into()).unwrap_err();
    assert_eq!(err.code, ErrorCode::FileNotFound);

    // 5. Unscoped image file in arbitrary directory is rejected with PermissionDenied
    let outside_dir = temp.path().join("user_private_docs");
    std::fs::create_dir_all(&outside_dir).unwrap();
    let unpicked_img = outside_dir.join("personal.png");
    let img = image::RgbImage::new(1, 1);
    img.save(&unpicked_img).unwrap();

    let err = read_image_data_impl(&state, unpicked_img.to_str().unwrap().into()).unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionDenied);

    // 6. Staged file in staging_dir is allowed
    let staged_img = staging_dir.join("staged.png");
    img.save(&staged_img).unwrap();
    let data_url = read_image_data_impl(&state, staged_img.to_str().unwrap().into()).unwrap();
    assert!(data_url.starts_with("data:image/png;base64,"));

    // 7. Preview file in preview_cache_dir is allowed
    let preview_img = preview_dir.join("thumbnail.jpg");
    img.save(&preview_img).unwrap();
    let data_url = read_image_data_impl(&state, preview_img.to_str().unwrap().into()).unwrap();
    assert!(data_url.starts_with("data:image/jpeg;base64,"));

    // 8. User-picked image in arbitrary directory is allowed once allowed in preview_scope
    let picked_img = outside_dir.join("selected_for_upscale.png");
    img.save(&picked_img).unwrap();
    state.preview_scope.allow_file(&picked_img);
    let data_url = read_image_data_impl(&state, picked_img.to_str().unwrap().into()).unwrap();
    assert!(data_url.starts_with("data:image/png;base64,"));

    // 9. Sibling file in outside_dir is STILL denied (granular file-level scoping)
    let sibling_img = outside_dir.join("not_selected.png");
    img.save(&sibling_img).unwrap();
    let err = read_image_data_impl(&state, sibling_img.to_str().unwrap().into()).unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionDenied);

    // 10. Output directory allowed in preview_scope allows all output images in it
    let out_dir = temp.path().join("user_upscale_output");
    std::fs::create_dir_all(&out_dir).unwrap();
    state.preview_scope.allow_directory(&out_dir);

    let output_img = out_dir.join("job1_result.webp");
    img.save(&output_img).unwrap();
    let data_url = read_image_data_impl(&state, output_img.to_str().unwrap().into()).unwrap();
    assert!(data_url.starts_with("data:image/webp;base64,"));
}

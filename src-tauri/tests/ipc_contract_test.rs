use resvera_core::{
    atomic_save_image, OutputFormat, UpscaleJobRequest as CoreJobRequest,
};
use resvera_desktop::commands::*;
use resvera_desktop::ipc_types::*;
use resvera_engine_ort::OrtEngine;
use resvera_persistence::AppDatabase;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[test]
fn test_ipc_types_serialization_rules() {
    let fmt = OutputFormat::Webp {
        lossless: true,
        quality: None,
    };
    let json = serde_json::to_string(&fmt).unwrap();
    // Verify camelCase discriminator
    assert_eq!(json, "{\"kind\":\"webp\",\"lossless\":true,\"quality\":null}");

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
    let orchestrator = resvera_core::JobOrchestrator::new(
        db,
        engine,
        temp.path().join("previews"),
    );
    let state = AppState {
        orchestrator,
        settings: Arc::new(Mutex::new(AppSettings::default())),
    };

    // 1. Get runtime status
    let status = get_runtime_status_impl(&state).unwrap();
    assert!(status.offline_ready);
    assert_eq!(status.engine.id, "ort");

    // 2. List models
    let models = list_models();
    assert_eq!(models.len(), 4);
    assert_eq!(models[0].id, "realesrgan-x4plus");
    assert_eq!(models[2].id, "real-cugan-2x");

    // 3. Settings load and save
    let default_settings = load_settings_impl(&state);
    assert_eq!(default_settings.schema_version, 1);

    let mut new_settings = default_settings.clone();
    new_settings.theme = "dark".into();
    let saved = save_settings_impl(&state, new_settings);
    assert_eq!(saved.theme, "dark");

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
        tile_size: Some(16),
        provider_preference: Some("cpu".to_string()),
    };

    let snapshot = create_upscale_job_impl(&state, req).unwrap();
    assert_eq!(snapshot.state, "queued");

    let fetched = get_job_impl(&state, &snapshot.id).unwrap();
    assert_eq!(fetched.id, snapshot.id);
    assert_eq!(fetched.state, "queued");
}

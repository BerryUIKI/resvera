use image::{Rgb, RgbImage};
use resvera_core::{
    atomic_save_image, BatchJobDefaults, BatchJobRequest, JobOrchestrator, OutputFormat,
    UpscaleJobRequest,
};
use resvera_persistence::AppDatabase;
use std::sync::Arc;
use tempfile::tempdir;

fn create_test_image(path: &std::path::Path, width: u32, height: u32) {
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            img.put_pixel(x, y, Rgb([(x * 5) as u8, (y * 5) as u8, 128]));
        }
    }
    atomic_save_image(&img, path, &OutputFormat::Png, None).unwrap();
}

#[test]
fn test_single_job_orchestration() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(MockEngine);
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let input_path = temp.path().join("input.png");
    create_test_image(&input_path, 32, 32);

    let req = UpscaleJobRequest {
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

    let queued = orchestrator.submit_job(&req).unwrap();
    assert_eq!(queued.state, "queued");
    assert_eq!(queued.model_package_version, "1.0.0");
    assert_eq!(queued.provider_id.as_deref(), Some("cpu"));
    assert_eq!(queued.output_directory.as_deref(), temp.path().to_str());
    assert_eq!(queued.tile_size, Some(32));
    assert!(queued.output_format_json.is_some());

    let completed = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(completed.state, "succeeded");
    assert!(completed.output_path.is_some());
    assert!(std::path::Path::new(&completed.output_path.unwrap()).exists());
    assert!(completed.preview_path.is_some());
    assert!(std::path::Path::new(&completed.preview_path.unwrap()).exists());
}

#[test]
fn test_batch_jobs_and_pause_resume() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(MockEngine);
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus-anime", 4);
    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let in1 = temp.path().join("in1.png");
    let in2 = temp.path().join("in2.png");
    let in3 = temp.path().join("in3.png");
    create_test_image(&in1, 16, 16);
    create_test_image(&in2, 16, 16);
    create_test_image(&in3, 16, 16);

    let batch_req = BatchJobRequest {
        inputs: vec![
            in1.to_str().unwrap().to_string(),
            in2.to_str().unwrap().to_string(),
            in3.to_str().unwrap().to_string(),
        ],
        defaults: BatchJobDefaults {
            output_directory: temp.path().to_str().unwrap().to_string(),
            model_id: "realesrgan-x4plus-anime".to_string(),
            model_variant_id: "default".to_string(),
            target_scale: 4,
            output_format: OutputFormat::Png,
            overwrite: false,
            tile_size: Some(32),
            provider_preference: Some("cpu".to_string()),
        },
    };

    let submitted = orchestrator.submit_batch(&batch_req).unwrap();
    assert_eq!(submitted.len(), 3);

    // Process job 1
    let j1 = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(j1.state, "succeeded");

    // Pause queue
    orchestrator.pause_queue();
    assert!(orchestrator.is_paused());
    let paused_result = orchestrator.process_next_job().unwrap();
    assert!(paused_result.is_none());

    // Resume queue and process remaining 2
    orchestrator.resume_queue();
    let j2 = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(j2.state, "succeeded");
    let j3 = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(j3.state, "succeeded");

    // Queue empty
    let empty = orchestrator.process_next_job().unwrap();
    assert!(empty.is_none());
}

#[test]
fn test_100_job_stress_batch() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(MockEngine);
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let in_img = temp.path().join("stress_in.png");
    create_test_image(&in_img, 16, 16);

    let mut inputs = Vec::with_capacity(100);
    for _ in 0..100 {
        inputs.push(in_img.to_str().unwrap().to_string());
    }

    let batch = BatchJobRequest {
        inputs,
        defaults: BatchJobDefaults {
            output_directory: temp.path().to_str().unwrap().to_string(),
            model_id: "realesrgan-x4plus".to_string(),
            model_variant_id: "default".to_string(),
            target_scale: 4,
            output_format: OutputFormat::Png,
            overwrite: true,
            tile_size: Some(32),
            provider_preference: Some("cpu".to_string()),
        },
    };

    let submitted = orchestrator.submit_batch(&batch).unwrap();
    assert_eq!(submitted.len(), 100);

    for _ in 0..100 {
        let job = orchestrator.process_next_job().unwrap().unwrap();
        assert_eq!(job.state, "succeeded");
    }

    assert!(orchestrator.process_next_job().unwrap().is_none());
}

#[test]
fn test_submission_rejects_incompatible_model_options() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let orchestrator = JobOrchestrator::with_models_root(
        db,
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    );
    let input_path = temp.path().join("input.png");
    create_test_image(&input_path, 16, 16);

    let base = UpscaleJobRequest {
        input_path: input_path.to_string_lossy().into_owned(),
        output_directory: String::new(),
        model_id: "realesrgan-x4plus".into(),
        model_variant_id: "default".into(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        provider_preference: None,
    };

    let mut invalid = base.clone();
    invalid.target_scale = 5;
    assert!(orchestrator.submit_job(&invalid).is_err());

    invalid = base.clone();
    invalid.tile_size = Some(16);
    assert!(orchestrator.submit_job(&invalid).is_err());

    invalid = base.clone();
    invalid.provider_preference = Some("cuda".into());
    assert!(orchestrator.submit_job(&invalid).is_err());

    invalid = base;
    invalid.model_variant_id = "missing".into();
    assert!(orchestrator.submit_job(&invalid).is_err());
}
mod common;

use common::{install_mock_model, MockEngine};

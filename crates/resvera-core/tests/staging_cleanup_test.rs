mod common;

use common::{install_mock_model, MockEngine};
use image::{Rgb, RgbImage};
use resvera_core::{atomic_save_image, JobOrchestrator, OutputFormat, UpscaleJobRequest};
use resvera_persistence::{AppDatabase, JobRecord};
use std::path::Path;
use std::sync::Arc;
use tempfile::tempdir;

fn create_test_image(path: &Path, width: u32, height: u32) {
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            img.put_pixel(x, y, Rgb([(x * 10) as u8, (y * 10) as u8, 128]));
        }
    }
    atomic_save_image(&img, path, &OutputFormat::Png, None).unwrap();
}

#[test]
fn test_staged_input_cleaned_on_successful_job_completion() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    let staging_dir = temp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();

    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    )
    .with_staging_dir(&staging_dir);

    let staged_input = staging_dir.join("upload_123.png");
    create_test_image(&staged_input, 32, 32);
    assert!(staged_input.is_file());

    let out_dir = temp.path().join("outputs");
    std::fs::create_dir_all(&out_dir).unwrap();

    let req = UpscaleJobRequest {
        input_path: staged_input.to_str().unwrap().to_string(),
        output_directory: out_dir.to_str().unwrap().to_string(),
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

    let submitted = orchestrator.submit_job(&req).unwrap();
    assert_eq!(submitted.state, "queued");

    // Process the job
    let completed = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(completed.state, "succeeded");

    // The staged file must be removed after successful execution
    assert!(
        !staged_input.exists(),
        "Staged input file must be deleted after job success"
    );
}

#[test]
fn test_staged_input_cleaned_on_job_cancellation() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    let staging_dir = temp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();

    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    )
    .with_staging_dir(&staging_dir);

    let staged_input = staging_dir.join("cancel_test.png");
    create_test_image(&staged_input, 32, 32);
    assert!(staged_input.is_file());

    let out_dir = temp.path().join("outputs");
    std::fs::create_dir_all(&out_dir).unwrap();

    let req = UpscaleJobRequest {
        input_path: staged_input.to_str().unwrap().to_string(),
        output_directory: out_dir.to_str().unwrap().to_string(),
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

    let submitted = orchestrator.submit_job(&req).unwrap();

    // Cancel while queued
    orchestrator.cancel_job(&submitted.id).unwrap();

    // The staged file must be deleted on cancellation
    assert!(
        !staged_input.exists(),
        "Staged input file must be deleted after job cancellation"
    );
}

#[test]
fn test_staged_input_preserved_while_recoverable_job_exists() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    let staging_dir = temp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();

    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    )
    .with_staging_dir(&staging_dir);

    let staged_input = staging_dir.join("shared_input.png");
    create_test_image(&staged_input, 32, 32);

    let input_str = staged_input.to_str().unwrap().to_string();

    // Insert two jobs referencing the same staged input (e.g. original job cancelled, but a retried job is queued)
    let j1 = JobRecord {
        id: "job-1".to_string(),
        state: "completed".to_string(),
        input_path: input_str.clone(),
        output_path: None,
        preview_path: None,
        model_id: "realesrgan-x4plus".to_string(),
        model_package_version: "1.0.0".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        engine_id: "ort".to_string(),
        provider_id: Some("cpu".to_string()),
        progress_fraction: 1.0,
        progress_stage: "succeeded".to_string(),
        error_code: None,
        error_message: None,
        output_directory: None,
        output_format_json: None,
        overwrite: false,
        tile_size: None,
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        created_at: "2026-09-21T10:00:00Z".to_string(),
        updated_at: "2026-09-21T10:05:00Z".to_string(),
    };
    let j2 = JobRecord {
        id: "job-2".to_string(),
        state: "queued".to_string(), // recoverable queued state!
        input_path: input_str.clone(),
        output_path: None,
        preview_path: None,
        model_id: "realesrgan-x4plus".to_string(),
        model_package_version: "1.0.0".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        engine_id: "ort".to_string(),
        provider_id: Some("cpu".to_string()),
        progress_fraction: 0.0,
        progress_stage: "queued".to_string(),
        error_code: None,
        error_message: None,
        output_directory: None,
        output_format_json: None,
        overwrite: false,
        tile_size: None,
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        created_at: "2026-09-21T10:01:00Z".to_string(),
        updated_at: "2026-09-21T10:01:00Z".to_string(),
    };
    db.insert_job(&j1).unwrap();
    db.insert_job(&j2).unwrap();

    // Job 1 completes/cleans up, but Job 2 is still queued (recoverable)
    orchestrator.cleanup_staged_input_if_unreferenced(&input_str);
    assert!(
        staged_input.exists(),
        "Staged input must be preserved while job-2 is in queued state"
    );

    // When job-2 is cancelled
    orchestrator.cancel_job("job-2").unwrap();
    assert!(
        !staged_input.exists(),
        "Staged input must be deleted once no recoverable job references it"
    );
}

#[test]
fn test_startup_sweep_cleans_abandoned_and_preserves_recoverable() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    let staging_dir = temp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    )
    .with_staging_dir(&staging_dir);

    // 1. Abandoned file (upload that was never submitted)
    let abandoned_file = staging_dir.join("abandoned_upload.png");
    create_test_image(&abandoned_file, 32, 32);

    // 2. Referenced file by a queued job
    let referenced_file = staging_dir.join("queued_upload.png");
    create_test_image(&referenced_file, 32, 32);

    let j_queued = JobRecord {
        id: "job-queued-1".to_string(),
        state: "queued".to_string(),
        input_path: referenced_file.to_str().unwrap().to_string(),
        output_path: None,
        preview_path: None,
        model_id: "realesrgan-x4plus".to_string(),
        model_package_version: "1.0.0".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        engine_id: "ort".to_string(),
        provider_id: Some("cpu".to_string()),
        progress_fraction: 0.0,
        progress_stage: "queued".to_string(),
        error_code: None,
        error_message: None,
        output_directory: None,
        output_format_json: None,
        overwrite: false,
        tile_size: None,
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        created_at: "2026-09-21T10:00:00Z".to_string(),
        updated_at: "2026-09-21T10:00:00Z".to_string(),
    };
    db.insert_job(&j_queued).unwrap();

    // Run sweep
    let swept = orchestrator.sweep_abandoned_staging().unwrap();
    assert_eq!(swept, 1, "Exactly one abandoned file should be swept");

    assert!(
        !abandoned_file.exists(),
        "Abandoned staging file must be swept"
    );
    assert!(
        referenced_file.exists(),
        "Staged file referenced by queued job must be preserved"
    );
}

#[test]
fn test_user_non_staged_files_are_never_deleted() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    let staging_dir = temp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();

    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    )
    .with_staging_dir(&staging_dir);

    let user_file = temp.path().join("original_photo.png");
    create_test_image(&user_file, 32, 32);

    let out_dir = temp.path().join("outputs");
    std::fs::create_dir_all(&out_dir).unwrap();

    let req = UpscaleJobRequest {
        input_path: user_file.to_str().unwrap().to_string(),
        output_directory: out_dir.to_str().unwrap().to_string(),
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

    let _submitted = orchestrator.submit_job(&req).unwrap();
    orchestrator.process_next_job().unwrap().unwrap();

    // User's original file must NEVER be deleted
    assert!(
        user_file.exists(),
        "User file outside staging directory must never be deleted"
    );

    // Cancel test with user file
    let req2 = UpscaleJobRequest {
        input_path: user_file.to_str().unwrap().to_string(),
        output_directory: out_dir.to_str().unwrap().to_string(),
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
    let submitted2 = orchestrator.submit_job(&req2).unwrap();
    orchestrator.cancel_job(&submitted2.id).unwrap();

    assert!(
        user_file.exists(),
        "User file must never be deleted on job cancellation"
    );
}

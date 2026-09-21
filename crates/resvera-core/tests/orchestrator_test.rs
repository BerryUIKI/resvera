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
use resvera_core::{
    CancellationToken, EngineCapabilities, EngineError, EngineHealth, EngineId, InferenceEngine,
    ModelSession, OwnedTensor, TensorView,
};

struct FinalizeCancelEngine;

impl InferenceEngine for FinalizeCancelEngine {
    fn id(&self) -> EngineId {
        EngineId("test-cancel-engine".into())
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            engine_id: self.id(),
            supported_providers: vec!["cpu".into()],
            supports_fp16: false,
            supports_dynamic_shapes: true,
        }
    }

    fn probe(&self) -> Result<EngineHealth, EngineError> {
        Ok(EngineHealth {
            healthy: true,
            active_provider: "cpu".into(),
            diagnostic_message: None,
        })
    }

    fn load(
        &self,
        model_bytes: &[u8],
        preference: Option<&str>,
    ) -> Result<Box<dyn ModelSession>, EngineError> {
        MockEngine.load(model_bytes, preference)
    }

    fn run(
        &self,
        session: &mut dyn ModelSession,
        input: TensorView<'_>,
        cancel: &CancellationToken,
    ) -> Result<OwnedTensor, EngineError> {
        let out = MockEngine.run(session, input, cancel)?;
        // Trigger cancellation right as inference completes so finalization detects it.
        cancel.cancel();
        Ok(out)
    }
}

#[test]
fn test_cancellation_during_finalization_cleans_artifacts() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let preview_dir = temp.path().join("previews");
    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(FinalizeCancelEngine),
        preview_dir.clone(),
        &models_root,
    );

    let input_path = temp.path().join("cancel_input.png");
    create_test_image(&input_path, 32, 32);

    let output_dir = temp.path().join("outputs");
    let req = UpscaleJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: output_dir.to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: true,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };

    let queued = orchestrator.submit_job(&req).unwrap();
    assert_eq!(queued.state, "queued");

    // Processing will run inference and trigger cancellation right at the finalization boundary
    let processed = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(processed.state, "cancelled");

    // Verify neither target output nor preview thumbnail remain orphaned on disk
    let expected_output = output_dir.join("cancel_input_4x.png");
    assert!(
        !expected_output.exists(),
        "Target output file should have been cleaned up on cancellation"
    );

    let preview_file = preview_dir.join(format!("{}_preview.png", queued.id));
    assert!(
        !preview_file.exists(),
        "Preview file should have been cleaned up on cancellation"
    );

    // Database state remains strictly cancelled
    let db_job = db.get_job(&queued.id).unwrap().unwrap();
    assert_eq!(db_job.state, "cancelled");
}

#[test]
fn test_database_cancellation_race_during_finalizing_commits() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    );

    let input_path = temp.path().join("race_input.png");
    create_test_image(&input_path, 16, 16);

    let req = UpscaleJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: true,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };

    let queued = orchestrator.submit_job(&req).unwrap();
    assert_eq!(queued.state, "queued");

    // Claim and transition through running to finalizing
    let claimed = db.claim_next_queued_job().unwrap().unwrap();
    assert_eq!(claimed.id, queued.id);
    assert_eq!(claimed.state, "preparing");
    assert!(db
        .transition_job_state(&claimed.id, "preparing", "running")
        .unwrap());
    assert!(db
        .transition_job_state(&claimed.id, "running", "finalizing")
        .unwrap());

    // Concurrent cancel arrives while finalizing
    let cancelled = db.cancel_job(&claimed.id).unwrap();
    assert!(cancelled);

    // Completion commit should now fail due to state constraint
    let commit_res = db.update_job_success(&claimed.id, "/fake/out.png", "/fake/prev.png");
    assert!(commit_res.is_err());

    // Failure update cannot overwrite the terminal cancelled state
    let failure_applied = db
        .update_job_failure(&claimed.id, "processingFailed", "Error occurred")
        .unwrap();
    assert!(
        !failure_applied,
        "Failure update must not overwrite cancelled state"
    );

    // Job in database remains reliably cancelled
    let final_record = db.get_job(&claimed.id).unwrap().unwrap();
    assert_eq!(final_record.state, "cancelled");
}

#[test]
fn test_preflight_input_validation() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(MockEngine);
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let orchestrator =
        JobOrchestrator::with_models_root(db, engine, temp.path().join("previews"), &models_root);

    // 1. Non-existent file
    let req_nonexistent = UpscaleJobRequest {
        input_path: temp
            .path()
            .join("nonexistent.png")
            .to_str()
            .unwrap()
            .to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };
    assert!(orchestrator.submit_job(&req_nonexistent).is_err());

    // 2. Corrupt / fake image disguised as PNG (magic bytes mismatch / invalid signature)
    let fake_png = temp.path().join("fake.png");
    std::fs::write(&fake_png, b"This is not a real PNG image!").unwrap();
    let req_fake = UpscaleJobRequest {
        input_path: fake_png.to_str().unwrap().to_string(),
        ..req_nonexistent.clone()
    };
    let err_fake = orchestrator.submit_job(&req_fake).unwrap_err();
    assert!(
        err_fake.to_string().contains("File may be corrupted")
            || err_fake.to_string().contains("Invalid PNG signature")
            || err_fake.to_string().contains("Unsupported or unrecognized")
    );

    // 3. Unsupported format (BMP)
    let bmp_path = temp.path().join("test.bmp");
    let mut bmp_bytes = vec![0u8; 54];
    bmp_bytes[0] = b'B';
    bmp_bytes[1] = b'M';
    std::fs::write(&bmp_path, bmp_bytes).unwrap();
    let req_bmp = UpscaleJobRequest {
        input_path: bmp_path.to_str().unwrap().to_string(),
        ..req_nonexistent.clone()
    };
    let err_bmp = orchestrator.submit_job(&req_bmp).unwrap_err();
    assert!(
        err_bmp.to_string().contains("not supported")
            || err_bmp.to_string().contains("Only PNG, JPEG, and WebP")
            || err_bmp
                .to_string()
                .contains("Cannot determine image format")
    );

    // 4. Target scale 8x requested on 4x model (scale truthfulness)
    let valid_png = temp.path().join("valid.png");
    create_test_image(&valid_png, 16, 16);
    let req_scale_8 = UpscaleJobRequest {
        input_path: valid_png.to_str().unwrap().to_string(),
        target_scale: 8,
        ..req_nonexistent.clone()
    };
    let err_scale_8 = orchestrator.submit_job(&req_scale_8).unwrap_err();
    assert!(err_scale_8
        .to_string()
        .contains("Target scale 8x is unsupported by variant 'default' (native 4x)"));
}

#[test]
fn test_queue_action_semantics_cancellation_and_retry() {
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

    let input_path = temp.path().join("retry_test.png");
    create_test_image(&input_path, 16, 16);

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

    // 1. Submit initial job
    let job1 = orchestrator.submit_job(&req).unwrap();
    assert_eq!(job1.state, "queued");

    // 2. Prevent duplicate submission of active jobs for same input
    let dup_err = orchestrator.submit_job(&req).unwrap_err();
    assert!(dup_err.to_string().contains("already queued or processing"));

    // 3. Cannot retry an active job
    let retry_active_err = orchestrator.retry_job(&job1.id).unwrap_err();
    assert!(retry_active_err.to_string().contains("Cannot retry job"));

    // 4. Process job to completion (succeeded)
    let completed = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(completed.id, job1.id);
    assert_eq!(completed.state, "succeeded");

    // 5. Retry succeeded job: clones original immutable parameters faithfully
    let retried = orchestrator.retry_job(&job1.id).unwrap();
    assert_ne!(retried.id, job1.id);
    assert_eq!(retried.state, "queued");
    assert_eq!(retried.input_path, job1.input_path);
    assert_eq!(retried.model_id, job1.model_id);
    assert_eq!(retried.model_variant_id, job1.model_variant_id);
    assert_eq!(retried.target_scale, job1.target_scale);
    assert_eq!(retried.tile_size, job1.tile_size);
    assert_eq!(retried.output_format_json, job1.output_format_json);
    assert_eq!(retried.provider_id, job1.provider_id);

    // 6. Cannot duplicate or retry while retried job is active
    let retry_again_err = orchestrator.retry_job(&job1.id).unwrap_err();
    assert!(retry_again_err.to_string().contains("already processing"));

    // 7. Cancel retried job from queued state
    assert!(orchestrator.cancel_job(&retried.id).is_ok());
    let cancelled_rec = db.get_job(&retried.id).unwrap().unwrap();
    assert_eq!(cancelled_rec.state, "cancelled");

    // 8. Retrying cancelled job succeeds and re-enqueues
    let retried_after_cancel = orchestrator.retry_job(&retried.id).unwrap();
    assert_eq!(retried_after_cancel.state, "queued");
}

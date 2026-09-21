mod common;

use common::{install_mock_model, MockEngine};
use image::{Rgb, RgbImage};
use resvera_core::{
    atomic_save_image, CancellationToken, EngineCapabilities, EngineError, EngineHealth, EngineId,
    InferenceEngine, JobOrchestrator, ModelSession, OutputFormat, OwnedTensor, TensorView,
    UpscaleJobRequest,
};
use resvera_persistence::AppDatabase;
use std::any::Any;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
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

// 1. Shutdown during Queued stage
#[test]
fn test_shutdown_during_queued_stage() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    );

    let in_path = temp.path().join("input1.png");
    create_test_image(&in_path, 32, 32);

    let req = UpscaleJobRequest {
        input_path: in_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".into(),
        model_variant_id: "default".into(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: None,
    };

    let queued_job = orchestrator.submit_job(&req).unwrap();
    assert_eq!(queued_job.state, "queued");

    // Coordinated application shutdown
    let clean = orchestrator.shutdown(Duration::from_millis(500)).unwrap();
    assert!(clean);
    assert!(orchestrator.is_paused());

    // Queued jobs remain queued and recoverable in DB
    let retrieved = db.get_job(&queued_job.id).unwrap().unwrap();
    assert_eq!(retrieved.state, "queued");

    // Paused orchestrator will not claim new jobs while shutdown
    let next = orchestrator.process_next_job().unwrap();
    assert!(next.is_none());
}

// 2. Shutdown during Preprocessing stage
#[test]
fn test_shutdown_during_preprocessing_stage() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    );

    let in_path = temp.path().join("input_prep.png");
    create_test_image(&in_path, 32, 32);

    let req = UpscaleJobRequest {
        input_path: in_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".into(),
        model_variant_id: "default".into(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: None,
    };

    let job = orchestrator.submit_job(&req).unwrap();
    // Claim the job so it transitions into preparing
    let claimed = db.claim_next_queued_job().unwrap().unwrap();
    assert_eq!(claimed.state, "preparing");

    // Trigger shutdown during preparing stage
    let clean = orchestrator.shutdown(Duration::from_millis(500)).unwrap();
    assert!(clean);

    // Job state should be persisted cleanly without partial outputs
    let retrieved = db.get_job(&job.id).unwrap().unwrap();
    assert_eq!(retrieved.state, "interrupted");
    assert!(retrieved.output_path.is_none());
}

// 3. Shutdown during Inference stage
struct InferenceHookEngine {
    tile_started: Arc<AtomicBool>,
    continue_run: Arc<AtomicBool>,
}

struct HookSession;
impl ModelSession for HookSession {
    fn input_shape(&self) -> Option<[usize; 4]> {
        None
    }
    fn output_shape(&self) -> Option<[usize; 4]> {
        None
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl InferenceEngine for InferenceHookEngine {
    fn id(&self) -> EngineId {
        EngineId("inference-hook-mock".into())
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

    fn load(&self, _bytes: &[u8], _p: Option<&str>) -> Result<Box<dyn ModelSession>, EngineError> {
        Ok(Box::new(HookSession))
    }

    fn run(
        &self,
        _session: &mut dyn ModelSession,
        input: TensorView<'_>,
        cancel: &CancellationToken,
    ) -> Result<OwnedTensor, EngineError> {
        self.tile_started.store(true, Ordering::SeqCst);

        // Wait until cancellation or signal
        while !self.continue_run.load(Ordering::SeqCst) {
            if cancel.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        cancel.check()?;
        let [b, c, h, w] = input.shape;
        let out_h = h * 4;
        let out_w = w * 4;
        OwnedTensor::new([b, c, out_h, out_w], vec![0.0f32; b * c * out_h * out_w])
    }
}

#[test]
fn test_shutdown_during_inference_stage() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let tile_started = Arc::new(AtomicBool::new(false));
    let continue_run = Arc::new(AtomicBool::new(false));

    let engine = Arc::new(InferenceHookEngine {
        tile_started: Arc::clone(&tile_started),
        continue_run: Arc::clone(&continue_run),
    });

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let in_path = temp.path().join("input_inference.png");
    create_test_image(&in_path, 64, 64);

    let out_dir = temp.path().join("outputs");
    std::fs::create_dir_all(&out_dir).unwrap();

    let req = UpscaleJobRequest {
        input_path: in_path.to_str().unwrap().to_string(),
        output_directory: out_dir.to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".into(),
        model_variant_id: "default".into(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: None,
    };

    let job = orchestrator.submit_job(&req).unwrap();

    // Spawn processing in separate thread
    let orch_clone = orchestrator.clone();
    let worker_handle = std::thread::spawn(move || orch_clone.process_next_job());

    // Wait until inference tile begins
    while !tile_started.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(10));
    }

    // Coordinated application shutdown
    let clean = orchestrator.shutdown(Duration::from_secs(2)).unwrap();
    assert!(clean);

    let result = worker_handle.join().unwrap().unwrap().unwrap();
    assert_eq!(result.state, "cancelled");

    let retrieved = db.get_job(&job.id).unwrap().unwrap();
    assert_eq!(retrieved.state, "cancelled");

    // No partial output committed
    let outputs: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        outputs.is_empty(),
        "No partial outputs should be committed on cancel"
    );
}

// 4. Shutdown during Finalization stage
#[test]
fn test_shutdown_during_finalization_stage() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        Arc::new(MockEngine),
        temp.path().join("previews"),
        &models_root,
    );

    let in_path = temp.path().join("input_finalizing.png");
    create_test_image(&in_path, 32, 32);

    let out_dir = temp.path().join("outputs");
    std::fs::create_dir_all(&out_dir).unwrap();

    let req = UpscaleJobRequest {
        input_path: in_path.to_str().unwrap().to_string(),
        output_directory: out_dir.to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".into(),
        model_variant_id: "default".into(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: None,
    };

    let job = orchestrator.submit_job(&req).unwrap();
    let claimed = db.claim_next_queued_job().unwrap().unwrap();
    // Simulate transitioning into finalizing
    db.transition_job_state(&claimed.id, "preparing", "running")
        .unwrap();
    db.transition_job_state(&claimed.id, "running", "finalizing")
        .unwrap();

    // Trigger shutdown while in finalizing
    let clean = orchestrator.shutdown(Duration::from_millis(500)).unwrap();
    assert!(clean);

    // Should be swept to interrupted
    let retrieved = db.get_job(&job.id).unwrap().unwrap();
    assert_eq!(retrieved.state, "interrupted");

    // Output directory contains no committed files
    let outputs: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(outputs.is_empty());
}

// 5. Uninterruptible native inference with bounded timeout
struct UninterruptibleMockEngine {
    call_count: Arc<AtomicUsize>,
}

impl InferenceEngine for UninterruptibleMockEngine {
    fn id(&self) -> EngineId {
        EngineId("uninterruptible-mock".into())
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

    fn load(&self, _bytes: &[u8], _p: Option<&str>) -> Result<Box<dyn ModelSession>, EngineError> {
        Ok(Box::new(HookSession))
    }

    fn run(
        &self,
        _session: &mut dyn ModelSession,
        input: TensorView<'_>,
        _cancel: &CancellationToken,
    ) -> Result<OwnedTensor, EngineError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        // Simulate a native library call that blocks and does NOT check cancellation
        std::thread::sleep(Duration::from_millis(1500));

        let [b, c, h, w] = input.shape;
        let out_h = h * 4;
        let out_w = w * 4;
        OwnedTensor::new([b, c, out_h, out_w], vec![0.0f32; b * c * out_h * out_w])
    }
}

#[test]
fn test_shutdown_uninterruptible_inference_bounded_timeout() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::open(temp.path().join("test.db")).unwrap();
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);

    let call_count = Arc::new(AtomicUsize::new(0));
    let engine = Arc::new(UninterruptibleMockEngine {
        call_count: Arc::clone(&call_count),
    });

    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let in_path = temp.path().join("input_blocked.png");
    create_test_image(&in_path, 32, 32);

    let req = UpscaleJobRequest {
        input_path: in_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".into(),
        model_variant_id: "default".into(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: false,
        tile_size: Some(32),
        tile_overlap: None,
        blend_mode: None,
        naming_template: None,
        provider_preference: None,
    };

    let job = orchestrator.submit_job(&req).unwrap();

    let orch_clone = orchestrator.clone();
    let _worker = std::thread::spawn(move || {
        let _ = orch_clone.process_next_job();
    });

    // Wait until engine is executing
    while call_count.load(Ordering::SeqCst) == 0 {
        std::thread::sleep(Duration::from_millis(10));
    }

    let start = std::time::Instant::now();
    // Bounded shutdown timeout of 150ms (well under the 1500ms sleep of the engine)
    let clean = orchestrator.shutdown(Duration::from_millis(150)).unwrap();
    let elapsed = start.elapsed();

    // Must not have waited for the full 1500ms
    assert!(!clean, "Should indicate timed out / forced shutdown");
    assert!(
        elapsed < Duration::from_millis(500),
        "Shutdown must bound its wait latency, took {:?}",
        elapsed
    );

    // Persistence sweep must have run and marked in-flight job as interrupted
    let retrieved = db.get_job(&job.id).unwrap().unwrap();
    assert_eq!(retrieved.state, "interrupted");
}

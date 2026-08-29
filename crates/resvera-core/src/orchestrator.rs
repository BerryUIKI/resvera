use crate::adapter::{ModelAdapter, PipelineError, RrdbAdapter};
use crate::engine::{CancellationToken, EngineError, InferenceEngine};
use crate::pipeline::atomic::{atomic_save_image, generate_output_path};
use crate::pipeline::io::{load_image, OutputFormat};
use crate::pipeline::resample::downsample_lanczos3;
use crate::pipeline::tiling::{TileBlender, TilePlan};
use image::RgbImage;
use resvera_persistence::{AppDatabase, DatabaseError, JobRecord};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),
    #[error("Pipeline error: {0}")]
    Pipeline(#[from] PipelineError),
    #[error("Engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Job not found: {0}")]
    JobNotFound(String),
    #[error("Cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpscaleJobRequest {
    pub input_path: String,
    pub output_directory: String,
    pub model_id: String,
    pub model_variant_id: String,
    pub target_scale: u32,
    pub output_format: OutputFormat,
    pub overwrite: bool,
    pub tile_size: Option<u32>,
    pub provider_preference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJobRequest {
    pub inputs: Vec<String>,
    pub defaults: BatchJobDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJobDefaults {
    pub output_directory: String,
    pub model_id: String,
    pub model_variant_id: String,
    pub target_scale: u32,
    pub output_format: OutputFormat,
    pub overwrite: bool,
    pub tile_size: Option<u32>,
    pub provider_preference: Option<String>,
}

#[derive(Clone)]
pub struct JobOrchestrator {
    pub db: AppDatabase,
    pub engine: Arc<dyn InferenceEngine>,
    pub preview_cache_dir: PathBuf,
    paused: Arc<AtomicBool>,
    active_job_id: Arc<Mutex<Option<String>>>,
    active_cancel_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl JobOrchestrator {
    pub fn new<P: AsRef<Path>>(
        db: AppDatabase,
        engine: Arc<dyn InferenceEngine>,
        preview_cache_dir: P,
    ) -> Self {
        let preview_cache_dir = preview_cache_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&preview_cache_dir).ok();

        Self {
            db,
            engine,
            preview_cache_dir,
            paused: Arc::new(AtomicBool::new(false)),
            active_job_id: Arc::new(Mutex::new(None)),
            active_cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn pause_queue(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume_queue(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn submit_job(&self, req: &UpscaleJobRequest) -> Result<JobRecord, OrchestratorError> {
        let path = Path::new(&req.input_path);
        if !path.exists() {
            return Err(OrchestratorError::Validation(format!(
                "Input file not found: {}",
                req.input_path
            )));
        }

        let id = format!("job-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();

        let record = JobRecord {
            id: id.clone(),
            state: "queued".to_string(),
            input_path: req.input_path.clone(),
            output_path: None,
            preview_path: None,
            model_id: req.model_id.clone(),
            model_package_version: "1.0.0".to_string(),
            model_variant_id: req.model_variant_id.clone(),
            target_scale: req.target_scale,
            engine_id: self.engine.id().0,
            provider_id: req.provider_preference.clone(),
            progress_fraction: 0.0,
            progress_stage: "queued".to_string(),
            error_code: None,
            error_message: None,
            created_at: now.clone(),
            updated_at: now,
        };

        self.db.insert_job(&record)?;
        Ok(record)
    }

    pub fn submit_batch(&self, req: &BatchJobRequest) -> Result<Vec<JobRecord>, OrchestratorError> {
        let mut records = Vec::with_capacity(req.inputs.len());
        let now = chrono::Utc::now().to_rfc3339();

        for input in &req.inputs {
            let path = Path::new(input);
            if !path.exists() {
                return Err(OrchestratorError::Validation(format!(
                    "Input file not found: {}",
                    input
                )));
            }

            let id = format!("job-{}", uuid::Uuid::new_v4());
            records.push(JobRecord {
                id,
                state: "queued".to_string(),
                input_path: input.clone(),
                output_path: None,
                preview_path: None,
                model_id: req.defaults.model_id.clone(),
                model_package_version: "1.0.0".to_string(),
                model_variant_id: req.defaults.model_variant_id.clone(),
                target_scale: req.defaults.target_scale,
                engine_id: self.engine.id().0,
                provider_id: req.defaults.provider_preference.clone(),
                progress_fraction: 0.0,
                progress_stage: "queued".to_string(),
                error_code: None,
                error_message: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }

        self.db.insert_batch_jobs(&records)?;
        Ok(records)
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<(), OrchestratorError> {
        if let Some(token) = self.active_cancel_tokens.lock().unwrap().get(job_id) {
            token.cancel();
        }
        self.db.update_job_state(job_id, "cancelled")?;
        Ok(())
    }

    /// Fetches the next queued job and processes it synchronously.
    pub fn process_next_job(&self) -> Result<Option<JobRecord>, OrchestratorError> {
        if self.is_paused() {
            return Ok(None);
        }

        // Find next queued job
        let next_job = match self.find_oldest_queued_job()? {
            Some(j) => j,
            None => return Ok(None),
        };

        let job_id = next_job.id.clone();
        let cancel_token = CancellationToken::new();

        {
            *self.active_job_id.lock().unwrap() = Some(job_id.clone());
            self.active_cancel_tokens
                .lock()
                .unwrap()
                .insert(job_id.clone(), cancel_token.clone());
        }

        let result = self.execute_job(&next_job, &cancel_token);

        {
            *self.active_job_id.lock().unwrap() = None;
            self.active_cancel_tokens.lock().unwrap().remove(&job_id);
        }

        match result {
            Ok(completed) => Ok(Some(completed)),
            Err(OrchestratorError::Engine(EngineError::Cancelled)) | Err(OrchestratorError::Cancelled) => {
                self.db.update_job_state(&job_id, "cancelled")?;
                let mut updated = next_job;
                updated.state = "cancelled".to_string();
                Ok(Some(updated))
            }
            Err(e) => {
                self.db.update_job_state(&job_id, "failed")?;
                let mut updated = next_job;
                updated.state = "failed".to_string();
                updated.error_message = Some(e.to_string());
                Ok(Some(updated))
            }
        }
    }

    fn find_oldest_queued_job(&self) -> Result<Option<JobRecord>, OrchestratorError> {
        Ok(self.db.get_job_by_state("queued")?)
    }

    fn execute_job(
        &self,
        job: &JobRecord,
        cancel: &CancellationToken,
    ) -> Result<JobRecord, OrchestratorError> {
        cancel.check()?;

        // 1. Preparing
        self.db.update_job_state(&job.id, "preparing")?;
        let src_img = load_image(&job.input_path)?;
        let (width, height) = src_img.dimensions();

        let tile_size = 32u32;
        let overlap = 8u32;
        let plan = TilePlan::build(width, height, tile_size, overlap);
        let total_tiles = plan.tiles.len();

        // 2. Load model session
        let adapter = RrdbAdapter;
        let mut session = self
            .engine
            .load(b"model_bytes", job.provider_id.as_deref())?;

        // 3. Running inference tile by tile
        self.db.update_job_state(&job.id, "running")?;
        let native_scale = 4u32;
        let mut blender = TileBlender::new(width, height, native_scale);

        for (idx, tile_rect) in plan.tiles.iter().enumerate() {
            cancel.check()?;

            let mut tile_img = RgbImage::new(tile_rect.width, tile_rect.height);
            for ty in 0..tile_rect.height {
                for tx in 0..tile_rect.width {
                    tile_img.put_pixel(
                        tx,
                        ty,
                        *src_img.get_pixel(tile_rect.x + tx, tile_rect.y + ty),
                    );
                }
            }

            let in_tensor = adapter.preprocess(&tile_img)?;
            let out_tensor = self.engine.run(&mut *session, in_tensor.view(), cancel)?;
            let out_tile = adapter.postprocess(&out_tensor)?;

            blender.blend_tile(tile_rect, &out_tile, plan.overlap);

            let _fraction = ((idx + 1) as f32) / (total_tiles as f32);
            // Non-blocking progress update
        }

        // 4. Finalizing
        cancel.check()?;
        self.db.update_job_state(&job.id, "finalizing")?;

        let mut output_img = blender.finalize();

        // Handle target scale (e.g. 2x requested on a 4x native model -> Lanczos3 downsample)
        if job.target_scale < native_scale {
            let target_w = (width * job.target_scale).max(1);
            let target_h = (height * job.target_scale).max(1);
            output_img = downsample_lanczos3(&output_img, target_w, target_h)?;
        }

        // Generate output path
        let out_dir = Path::new(&job.input_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let target_path = generate_output_path(
            out_dir,
            Path::new(&job.input_path),
            &job.model_id,
            job.target_scale,
            &OutputFormat::Png,
            false,
        );

        // Atomic file write
        atomic_save_image(&output_img, &target_path, &OutputFormat::Png, Some(Path::new(&job.input_path)))?;

        // Generate cache-scoped preview thumbnail (max 256x256)
        let preview_name = format!("{}_preview.png", job.id);
        let preview_path = self.preview_cache_dir.join(preview_name);
        let (pw, ph) = (
            (output_img.width() / 4).max(1),
            (output_img.height() / 4).max(1),
        );
        let preview_img = downsample_lanczos3(&output_img, pw, ph)?;
        atomic_save_image(&preview_img, &preview_path, &OutputFormat::Png, None)?;

        // Update DB record to succeeded
        self.db.update_job_success(
            &job.id,
            target_path.to_str().unwrap(),
            preview_path.to_str().unwrap(),
        )?;

        let mut completed_job = job.clone();
        completed_job.state = "succeeded".to_string();
        completed_job.output_path = Some(target_path.to_str().unwrap().to_string());
        completed_job.preview_path = Some(preview_path.to_str().unwrap().to_string());
        completed_job.progress_fraction = 1.0;
        completed_job.progress_stage = "finalizing".to_string();

        Ok(completed_job)
    }
}

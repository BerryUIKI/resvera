use crate::adapter::{
    CuganAdapter, HatAdapter, ModelAdapter, PipelineError, RrdbAdapter, TileConstraints,
};
use crate::engine::{CancellationToken, EngineError, InferenceEngine};
use crate::pipeline::atomic::{
    atomic_save_image, atomic_save_image_with_alpha, generate_output_path,
};
use crate::pipeline::io::{load_image_with_alpha, OutputFormat};
use crate::pipeline::resample::downsample_lanczos3;
use crate::pipeline::tiling::{TileBlender, TilePlan};
use image::RgbImage;
use resvera_models::{InstallerError, ModelInstaller, ModelManifest, ResolvedModel};
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
    #[error("Model registry error: {0}")]
    Model(#[from] InstallerError),
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
    pub models_root: PathBuf,
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
        let models_root = default_models_root();
        Self::with_models_root(db, engine, preview_cache_dir, models_root)
    }

    pub fn with_models_root<P: AsRef<Path>, M: AsRef<Path>>(
        db: AppDatabase,
        engine: Arc<dyn InferenceEngine>,
        preview_cache_dir: P,
        models_root: M,
    ) -> Self {
        let preview_cache_dir = preview_cache_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&preview_cache_dir).ok();

        Self {
            db,
            engine,
            preview_cache_dir,
            models_root: models_root.as_ref().to_path_buf(),
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
        validate_input_file(&req.input_path)?;
        let provider = normalize_provider(req.provider_preference.as_deref())?;
        let resolved = self.resolve_request_model(
            &req.model_id,
            &req.model_variant_id,
            req.target_scale,
            req.tile_size,
            &provider,
        )?;

        let id = format!("job-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        let format_json = Some(serde_json::to_string(&req.output_format).map_err(|error| {
            OrchestratorError::Validation(format!("Could not serialize output format: {error}"))
        })?);

        let record = JobRecord {
            id: id.clone(),
            state: "queued".to_string(),
            input_path: req.input_path.clone(),
            output_path: None,
            preview_path: None,
            model_id: req.model_id.clone(),
            model_package_version: resolved.manifest.package_version,
            model_variant_id: req.model_variant_id.clone(),
            target_scale: req.target_scale,
            engine_id: self.engine.id().0,
            provider_id: Some(provider),
            progress_fraction: 0.0,
            progress_stage: "queued".to_string(),
            error_code: None,
            error_message: None,
            output_directory: Some(req.output_directory.clone()),
            output_format_json: format_json,
            overwrite: req.overwrite,
            tile_size: req.tile_size,
            created_at: now.clone(),
            updated_at: now,
        };

        self.db.insert_job(&record)?;
        Ok(record)
    }

    pub fn submit_batch(&self, req: &BatchJobRequest) -> Result<Vec<JobRecord>, OrchestratorError> {
        if req.inputs.is_empty() {
            return Err(OrchestratorError::Validation(
                "Batch must contain at least one input".into(),
            ));
        }
        let provider = normalize_provider(req.defaults.provider_preference.as_deref())?;
        let resolved = self.resolve_request_model(
            &req.defaults.model_id,
            &req.defaults.model_variant_id,
            req.defaults.target_scale,
            req.defaults.tile_size,
            &provider,
        )?;
        let mut records = Vec::with_capacity(req.inputs.len());
        let now = chrono::Utc::now().to_rfc3339();
        let format_json = Some(serde_json::to_string(&req.defaults.output_format).map_err(
            |error| {
                OrchestratorError::Validation(format!("Could not serialize output format: {error}"))
            },
        )?);

        for input in &req.inputs {
            validate_input_file(input)?;

            let id = format!("job-{}", uuid::Uuid::new_v4());
            records.push(JobRecord {
                id,
                state: "queued".to_string(),
                input_path: input.clone(),
                output_path: None,
                preview_path: None,
                model_id: req.defaults.model_id.clone(),
                model_package_version: resolved.manifest.package_version.clone(),
                model_variant_id: req.defaults.model_variant_id.clone(),
                target_scale: req.defaults.target_scale,
                engine_id: self.engine.id().0,
                provider_id: Some(provider.clone()),
                progress_fraction: 0.0,
                progress_stage: "queued".to_string(),
                error_code: None,
                error_message: None,
                output_directory: Some(req.defaults.output_directory.clone()),
                output_format_json: format_json.clone(),
                overwrite: req.defaults.overwrite,
                tile_size: req.defaults.tile_size,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }

        self.db.insert_batch_jobs(&records)?;
        Ok(records)
    }

    fn resolve_request_model(
        &self,
        model_id: &str,
        variant_id: &str,
        target_scale: u32,
        tile_size: Option<u32>,
        provider: &str,
    ) -> Result<ResolvedModel, OrchestratorError> {
        let resolved =
            ModelInstaller::new(&self.models_root).resolve_active_variant(model_id, variant_id)?;
        let adapter = adapter_for_manifest(&resolved.manifest, resolved.variant.native_scale)?;
        adapter.validate_manifest(&resolved.manifest)?;
        validated_tile_size(tile_size, adapter.tile_constraints(&resolved.manifest))?;

        if target_scale == 0 || target_scale > resolved.variant.native_scale {
            return Err(OrchestratorError::Validation(format!(
                "Target scale {target_scale}x is unsupported by variant '{}' (native {}x)",
                resolved.variant.id, resolved.variant.native_scale
            )));
        }

        let capabilities = self.engine.capabilities();
        if !capabilities
            .supported_providers
            .iter()
            .any(|supported| supported.eq_ignore_ascii_case(provider))
        {
            return Err(OrchestratorError::Validation(format!(
                "Provider '{provider}' is not available in the active engine"
            )));
        }
        if !resolved
            .manifest
            .compatibility
            .validated_providers
            .iter()
            .any(|validated| validated.eq_ignore_ascii_case(provider))
        {
            return Err(OrchestratorError::Validation(format!(
                "Provider '{provider}' has not been validated for model '{model_id}'"
            )));
        }

        Ok(resolved)
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<(), OrchestratorError> {
        if let Some(token) = self.active_cancel_tokens.lock().unwrap().get(job_id) {
            token.cancel();
        }
        if !self.db.cancel_job(job_id)? {
            let job = self
                .db
                .get_job(job_id)?
                .ok_or_else(|| OrchestratorError::JobNotFound(job_id.to_string()))?;
            return Err(OrchestratorError::Validation(format!(
                "Job {job_id} cannot be cancelled from terminal state '{}'",
                job.state
            )));
        }
        Ok(())
    }

    /// Fetches the next queued job and processes it synchronously.
    pub fn process_next_job(&self) -> Result<Option<JobRecord>, OrchestratorError> {
        if self.is_paused() {
            return Ok(None);
        }

        let next_job = match self.db.claim_next_queued_job()? {
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
            Err(OrchestratorError::Engine(EngineError::Cancelled))
            | Err(OrchestratorError::Cancelled) => {
                let _ = self.db.cancel_job(&job_id)?;
                Ok(self.db.get_job(&job_id)?)
            }
            Err(e) => {
                let _ = self
                    .db
                    .update_job_failure(&job_id, "processingFailed", &e.to_string())?;
                Ok(self.db.get_job(&job_id)?)
            }
        }
    }

    fn execute_job(
        &self,
        job: &JobRecord,
        cancel: &CancellationToken,
    ) -> Result<JobRecord, OrchestratorError> {
        cancel.check()?;

        let loaded_input = load_image_with_alpha(&job.input_path)?;
        let src_img = loaded_input.rgb;
        let src_alpha = loaded_input.alpha;
        let (width, height) = src_img.dimensions();

        let installer = ModelInstaller::new(&self.models_root);
        let resolved = installer.resolve_version_variant(
            &job.model_id,
            &job.model_package_version,
            &job.model_variant_id,
        )?;
        let adapter = adapter_for_manifest(&resolved.manifest, resolved.variant.native_scale)?;
        adapter.validate_manifest(&resolved.manifest)?;
        let constraints = adapter.tile_constraints(&resolved.manifest);
        let tile_size = validated_tile_size(job.tile_size, constraints)?;
        let overlap = constraints.overlap;
        let plan = TilePlan::build(width, height, tile_size, overlap);
        let total_tiles = plan.tiles.len();
        if total_tiles == 0 {
            return Err(OrchestratorError::Validation(
                "Input image produced an empty tile plan".into(),
            ));
        }

        let model_bytes = std::fs::read(&resolved.artifact_path)?;
        let mut session = self.engine.load(&model_bytes, job.provider_id.as_deref())?;

        if !self
            .db
            .transition_job_state(&job.id, "preparing", "running")?
        {
            return Err(OrchestratorError::Cancelled);
        }
        let native_scale = resolved.variant.native_scale;
        let mut blender = TileBlender::try_new(width, height, native_scale)?;

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
            let mut out_tile = adapter.postprocess(&out_tensor)?;
            let expected_width = tile_rect.width.checked_mul(native_scale).ok_or_else(|| {
                OrchestratorError::Validation("Upscaled tile width overflowed".into())
            })?;
            let expected_height = tile_rect.height.checked_mul(native_scale).ok_or_else(|| {
                OrchestratorError::Validation("Upscaled tile height overflowed".into())
            })?;
            if out_tile.width() < expected_width || out_tile.height() < expected_height {
                return Err(OrchestratorError::Pipeline(
                    PipelineError::DimensionMismatch(format!(
                        "Model produced {}x{} for a tile requiring {}x{}",
                        out_tile.width(),
                        out_tile.height(),
                        expected_width,
                        expected_height
                    )),
                ));
            }
            if out_tile.width() != expected_width || out_tile.height() != expected_height {
                out_tile =
                    image::imageops::crop_imm(&out_tile, 0, 0, expected_width, expected_height)
                        .to_image();
            }

            blender.blend_tile(tile_rect, &out_tile, plan.overlap);

            let fraction = ((idx + 1) as f32) / (total_tiles as f32);
            let stage_msg = format!("inferencing (tile {}/{})", idx + 1, total_tiles);
            if !self
                .db
                .update_job_progress(&job.id, fraction * 0.9, &stage_msg)?
            {
                return Err(OrchestratorError::Cancelled);
            }
        }

        // 4. Finalizing
        cancel.check()?;
        if !self
            .db
            .transition_job_state(&job.id, "running", "finalizing")?
        {
            return Err(OrchestratorError::Cancelled);
        }

        let mut output_img = blender.finalize();

        // Resample alpha mask if present to match native upscale dimensions
        let mut output_alpha = if let Some(ref alpha) = src_alpha {
            let native_w = width
                .checked_mul(native_scale)
                .ok_or_else(|| OrchestratorError::Validation("Upscaled width overflowed".into()))?;
            let native_h = height.checked_mul(native_scale).ok_or_else(|| {
                OrchestratorError::Validation("Upscaled height overflowed".into())
            })?;
            Some(crate::pipeline::resample::resample_alpha_lanczos3(
                alpha, native_w, native_h,
            )?)
        } else {
            None
        };

        // Handle target scale (e.g. 2x requested on a 4x native model -> Lanczos3 downsample)
        if job.target_scale < native_scale {
            let target_w = width.checked_mul(job.target_scale).ok_or_else(|| {
                OrchestratorError::Validation("Requested output width overflowed".into())
            })?;
            let target_h = height.checked_mul(job.target_scale).ok_or_else(|| {
                OrchestratorError::Validation("Requested output height overflowed".into())
            })?;
            output_img = downsample_lanczos3(&output_img, target_w, target_h)?;
            if let Some(ref a) = output_alpha {
                output_alpha = Some(crate::pipeline::resample::resample_alpha_lanczos3(
                    a, target_w, target_h,
                )?);
            }
        }

        // Parse configured output format
        let serialized_format = job.output_format_json.as_deref().ok_or_else(|| {
            OrchestratorError::Validation("Job is missing its output format".into())
        })?;
        let output_format: OutputFormat =
            serde_json::from_str(serialized_format).map_err(|error| {
                OrchestratorError::Validation(format!("Job has an invalid output format: {error}"))
            })?;

        // Generate output path
        let out_dir_buf = job
            .output_directory
            .as_deref()
            .filter(|directory| !directory.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(&job.input_path)
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            });

        let target_path = generate_output_path(
            &out_dir_buf,
            Path::new(&job.input_path),
            &job.model_id,
            job.target_scale,
            &output_format,
            job.overwrite,
        );

        // Atomic file write with alpha channel
        atomic_save_image_with_alpha(
            &output_img,
            output_alpha.as_ref(),
            &target_path,
            &output_format,
            Some(Path::new(&job.input_path)),
        )?;
        if let Err(error) = cancel.check() {
            let _ = std::fs::remove_file(&target_path);
            return Err(error.into());
        }

        // Generate cache-scoped preview thumbnail (bounded to max 256x256)
        let preview_name = format!("{}_preview.png", job.id);
        let preview_path = self.preview_cache_dir.join(preview_name);
        let finalize_result = (|| -> Result<(String, String), OrchestratorError> {
            let max_dim = 256.0f32;
            let scale_factor = f32::min(
                max_dim / output_img.width() as f32,
                max_dim / output_img.height() as f32,
            )
            .min(1.0);
            let pw = ((output_img.width() as f32 * scale_factor).round() as u32).max(1);
            let ph = ((output_img.height() as f32 * scale_factor).round() as u32).max(1);
            let preview_img = downsample_lanczos3(&output_img, pw, ph)?;
            atomic_save_image(&preview_img, &preview_path, &OutputFormat::Png, None)?;
            cancel.check()?;

            let output_path = target_path
                .to_str()
                .ok_or_else(|| {
                    OrchestratorError::Validation(
                        "Output path cannot be represented as UTF-8".into(),
                    )
                })?
                .to_string();
            let preview_path_string = preview_path
                .to_str()
                .ok_or_else(|| {
                    OrchestratorError::Validation(
                        "Preview path cannot be represented as UTF-8".into(),
                    )
                })?
                .to_string();

            self.db
                .update_job_success(&job.id, &output_path, &preview_path_string)?;
            Ok((output_path, preview_path_string))
        })();

        let (output_path, preview_path_string) = match finalize_result {
            Ok(paths) => paths,
            Err(error) => {
                let _ = std::fs::remove_file(&target_path);
                let _ = std::fs::remove_file(&preview_path);
                return Err(error);
            }
        };

        let mut completed_job = job.clone();
        completed_job.state = "succeeded".to_string();
        completed_job.output_path = Some(output_path);
        completed_job.preview_path = Some(preview_path_string);
        completed_job.progress_fraction = 1.0;
        completed_job.progress_stage = "succeeded".to_string();

        Ok(completed_job)
    }
}

fn default_models_root() -> PathBuf {
    std::env::var_os("RESVERA_MODELS_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".resvera").join("models"))
        })
        .unwrap_or_else(|| PathBuf::from(".resvera").join("models"))
}

fn validate_input_file(input: &str) -> Result<(), OrchestratorError> {
    let path = Path::new(input);
    if input.trim().is_empty() || !path.is_file() {
        return Err(OrchestratorError::Validation(format!(
            "Input file not found or is not a regular file: {input}"
        )));
    }
    Ok(())
}

fn normalize_provider(provider: Option<&str>) -> Result<String, OrchestratorError> {
    let provider = provider.unwrap_or("cpu").trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Err(OrchestratorError::Validation(
            "Provider id must not be empty".into(),
        ));
    }
    Ok(provider)
}

fn adapter_for_manifest(
    manifest: &ModelManifest,
    native_scale: u32,
) -> Result<Box<dyn ModelAdapter>, OrchestratorError> {
    match manifest.family.as_str() {
        "rrdb" | "rrdb-6b" => Ok(Box::new(RrdbAdapter)),
        "cugan" | "real-cugan" => Ok(Box::new(CuganAdapter::new(native_scale))),
        "hat" | "real-hat" | "real-hat-gan" => Ok(Box::new(HatAdapter::new(
            native_scale,
            manifest.tiling.window_size.unwrap_or(16),
        ))),
        family => Err(OrchestratorError::Validation(format!(
            "Unsupported model family: {family}"
        ))),
    }
}

fn validated_tile_size(
    requested: Option<u32>,
    constraints: TileConstraints,
) -> Result<u32, OrchestratorError> {
    let tile_size = requested.unwrap_or(constraints.recommended);
    if tile_size < constraints.minimum
        || tile_size <= constraints.overlap
        || !tile_size.is_multiple_of(constraints.alignment)
    {
        return Err(OrchestratorError::Validation(format!(
            "Tile size {tile_size} violates model constraints: minimum {}, overlap {}, alignment {}",
            constraints.minimum, constraints.overlap, constraints.alignment
        )));
    }
    Ok(tile_size)
}

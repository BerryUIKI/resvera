use crate::adapter::{ModelAdapter, PipelineError, TileConstraints};
use crate::engine::{CancellationToken, InferenceEngine};
use crate::pipeline::resample::downsample_lanczos3;
use crate::pipeline::resample::resample_rgb_lanczos3;
use crate::pipeline::tiling::{BlendMode, TileBlender, TilePlan, MAX_BLEND_OUTPUT_PIXELS};
use image::RgbImage;
use std::sync::Arc;

/// One stage of a cascade: the weights to load plus the adapter and tiling
/// rules derived from that model's manifest.
pub struct CascadePass<'a> {
    pub adapter: &'a dyn ModelAdapter,
    pub weights: &'a [u8],
    pub constraints: TileConstraints,
    /// Native scale of the resolved variant, i.e. the ratio this pass applies.
    pub native_scale: u32,
}

/// Progress reporting for a multi-pass job.
///
/// `fraction` is the completed fraction of the *whole* job across all passes.
pub trait CascadeProgress {
    fn report(&mut self, fraction: f32, stage: &str) -> Result<(), PipelineError>;
}

/// No-op progress sink for callers that do not persist progress.
pub struct NoProgress;

impl CascadeProgress for NoProgress {
    fn report(&mut self, _fraction: f32, _stage: &str) -> Result<(), PipelineError> {
        Ok(())
    }
}

pub struct CascadePipeline {
    engine: Arc<dyn InferenceEngine>,
}

impl CascadePipeline {
    pub fn new(engine: Arc<dyn InferenceEngine>) -> Self {
        Self { engine }
    }

    /// Total scale factor of a pass chain, e.g. `[4, 4]` -> 16.
    pub fn total_scale(passes: &[CascadePass<'_>]) -> u32 {
        passes.iter().map(|pass| pass.native_scale).product()
    }

    /// Output dimensions a pass chain produces from `width` x `height`.
    pub fn output_dimensions(
        passes: &[CascadePass<'_>],
        width: u32,
        height: u32,
    ) -> Result<(u32, u32), PipelineError> {
        let total = Self::total_scale(passes);
        let w = width
            .checked_mul(total)
            .ok_or_else(|| PipelineError::Validation("Cascaded output width overflowed".into()))?;
        let h = height
            .checked_mul(total)
            .ok_or_else(|| PipelineError::Validation("Cascaded output height overflowed".into()))?;
        Ok((w, h))
    }

    /// Runs every pass in order and downsamples to `target_scale` with Lanczos3
    /// when the chain overshoots it.
    ///
    /// Two models at 4x produce 16x, which is then reduced to the requested 8x.
    /// This is why an 8x request costs far more than twice a 4x request: the peak
    /// working image is the *final pass* output, not the requested size.
    pub fn run(
        &self,
        src_img: &RgbImage,
        passes: &[CascadePass<'_>],
        target_scale: u32,
        blend_mode: BlendMode,
        cancel: &CancellationToken,
        progress: &mut dyn CascadeProgress,
    ) -> Result<RgbImage, PipelineError> {
        if passes.is_empty() {
            return Err(PipelineError::Validation(
                "A cascade requires at least one pass".into(),
            ));
        }
        if target_scale == 0 {
            return Err(PipelineError::Validation(
                "Target scale must be greater than zero".into(),
            ));
        }
        cancel.check()?;

        let (src_w, src_h) = src_img.dimensions();
        let total_scale = Self::total_scale(passes);
        let (full_w, full_h) = Self::output_dimensions(passes, src_w, src_h)?;

        // The peak working buffer is the full chain output, not the requested
        // size, because downsampling happens only after the last pass.
        let peak_pixels = (full_w as u64) * (full_h as u64);
        if peak_pixels > MAX_BLEND_OUTPUT_PIXELS {
            return Err(PipelineError::Validation(format!(
                "A {total_scale}x cascade from {src_w}x{src_h} needs a {full_w}x{full_h} working \
                 buffer ({peak_pixels} pixels), which exceeds the {MAX_BLEND_OUTPUT_PIXELS} pixel \
                 blending limit. Use a smaller input image or fewer cascade passes."
            )));
        }

        // Total tiles across all passes, so progress reflects the whole job
        // rather than restarting at each pass. Each pass is planned against the
        // previous pass's output dimensions.
        let mut pass_tiles: Vec<usize> = Vec::with_capacity(passes.len());
        let mut dimensions = (src_w, src_h);
        for pass in passes {
            let tile_size = default_tile_size(pass.constraints);
            let plan = TilePlan::build(
                dimensions.0,
                dimensions.1,
                tile_size,
                pass.constraints.overlap,
            );
            pass_tiles.push(plan.tiles.len());
            dimensions = (
                dimensions
                    .0
                    .checked_mul(pass.native_scale)
                    .ok_or_else(|| PipelineError::Validation("Cascaded width overflowed".into()))?,
                dimensions.1.checked_mul(pass.native_scale).ok_or_else(|| {
                    PipelineError::Validation("Cascaded height overflowed".into())
                })?,
            );
        }
        let total_units = pass_tiles.iter().sum::<usize>().max(1) as f32;

        let mut current = src_img.clone();
        let mut completed_units = 0.0f32;

        for (index, pass) in passes.iter().enumerate() {
            cancel.check()?;

            let (in_w, in_h) = current.dimensions();
            let tile_size = default_tile_size(pass.constraints);
            let overlap = pass.constraints.overlap;
            validate_tile_params(tile_size, overlap, pass.constraints, index)?;

            let plan = TilePlan::build(in_w, in_h, tile_size, overlap);
            if plan.tiles.is_empty() {
                return Err(PipelineError::Validation(format!(
                    "Cascade pass {} produced an empty tile plan",
                    index + 1
                )));
            }

            let mut session = self
                .engine
                .load(pass.weights, None)
                .map_err(PipelineError::from)?;
            let mut blender =
                TileBlender::try_new_with_blend_mode(in_w, in_h, pass.native_scale, blend_mode)?;

            for tile_rect in plan.tiles.iter() {
                cancel.check()?;

                let mut tile_img = RgbImage::new(tile_rect.width, tile_rect.height);
                for ty in 0..tile_rect.height {
                    for tx in 0..tile_rect.width {
                        tile_img.put_pixel(
                            tx,
                            ty,
                            *current.get_pixel(tile_rect.x + tx, tile_rect.y + ty),
                        );
                    }
                }

                let in_tensor = pass.adapter.preprocess(&tile_img)?;
                let out_tensor = self.engine.run(&mut *session, in_tensor.view(), cancel)?;
                let mut out_tile = pass.adapter.postprocess(&out_tensor)?;

                // Adapters may pad or crop; force the tile to the exact size the
                // blender expects before compositing.
                let expected_w = tile_rect
                    .width
                    .checked_mul(pass.native_scale)
                    .ok_or_else(|| PipelineError::Validation("Tile width overflowed".into()))?;
                let expected_h = tile_rect
                    .height
                    .checked_mul(pass.native_scale)
                    .ok_or_else(|| PipelineError::Validation("Tile height overflowed".into()))?;
                if out_tile.width() != expected_w || out_tile.height() != expected_h {
                    out_tile = resample_rgb_lanczos3(&out_tile, expected_w, expected_h)?;
                }

                blender.blend_tile(tile_rect, &out_tile, overlap);

                completed_units += 1.0;
                progress.report(
                    (completed_units / total_units) * 0.95,
                    &format!(
                        "cascade pass {}/{} (tile {}/{})",
                        index + 1,
                        passes.len(),
                        tile_rect.y / tile_size.max(1) + 1,
                        plan.tiles.len()
                    ),
                )?;
            }

            current = blender.finalize();
            cancel.check()?;
        }

        // The chain may overshoot the request (4x + 4x = 16x for an 8x job).
        if total_scale != target_scale {
            let tw = src_w.checked_mul(target_scale).ok_or_else(|| {
                PipelineError::Validation("Requested output width overflowed".into())
            })?;
            let th = src_h.checked_mul(target_scale).ok_or_else(|| {
                PipelineError::Validation("Requested output height overflowed".into())
            })?;
            cancel.check()?;
            current = if target_scale < total_scale {
                downsample_lanczos3(&current, tw, th)?
            } else {
                resample_rgb_lanczos3(&current, tw, th)?
            };
        }

        progress.report(1.0, "finalizing")?;
        Ok(current)
    }
}

/// Tile size for a pass, derived from the model's own constraints.
fn default_tile_size(constraints: TileConstraints) -> u32 {
    let requested = constraints.recommended.max(constraints.minimum);
    round_up_to_alignment(requested, constraints.alignment)
}

fn round_up_to_alignment(value: u32, alignment: u32) -> u32 {
    if alignment <= 1 {
        return value;
    }
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

fn validate_tile_params(
    tile_size: u32,
    overlap: u32,
    constraints: TileConstraints,
    pass_index: usize,
) -> Result<(), PipelineError> {
    if tile_size < constraints.minimum {
        return Err(PipelineError::Validation(format!(
            "Cascade pass {}: tile size {tile_size} is below the model minimum of {}",
            pass_index + 1,
            constraints.minimum
        )));
    }
    if overlap >= tile_size {
        return Err(PipelineError::Validation(format!(
            "Cascade pass {}: overlap {overlap} must be smaller than the tile size {tile_size}",
            pass_index + 1
        )));
    }
    Ok(())
}

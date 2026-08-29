use crate::adapter::{ModelAdapter, PipelineError};
use crate::engine::{CancellationToken, InferenceEngine};
use crate::pipeline::resample::downsample_lanczos3;
use crate::pipeline::tiling::{TileBlender, TilePlan};
use image::RgbImage;
use std::sync::Arc;

pub struct CascadePipeline {
    engine: Arc<dyn InferenceEngine>,
}

impl CascadePipeline {
    pub fn new(engine: Arc<dyn InferenceEngine>) -> Self {
        Self { engine }
    }

    /// Executes an 8x cascade: Pass 1 (4x upscale) followed by Pass 2 (2x upscale).
    pub fn run_8x_cascade(
        &self,
        src_img: &RgbImage,
        adapter_pass1: &dyn ModelAdapter,
        adapter_pass2: &dyn ModelAdapter,
        target_scale: u32,
        cancel: &CancellationToken,
    ) -> Result<RgbImage, PipelineError> {
        cancel.check()?;

        // --- PASS 1: 4x Super-Resolution ---
        let (w1, h1) = src_img.dimensions();
        let plan1 = TilePlan::build(w1, h1, 32, 8);
        let mut session1 = self.engine.load(b"pass1_bytes", None)?;
        let mut blender1 = TileBlender::new(w1, h1, 4);

        for tile in &plan1.tiles {
            cancel.check()?;
            let mut tile_img = RgbImage::new(tile.width, tile.height);
            for ty in 0..tile.height {
                for tx in 0..tile.width {
                    tile_img.put_pixel(tx, ty, *src_img.get_pixel(tile.x + tx, tile.y + ty));
                }
            }

            let in_tensor = adapter_pass1.preprocess(&tile_img)?;
            let out_tensor = self.engine.run(&mut *session1, in_tensor.view(), cancel)?;
            let out_tile = adapter_pass1.postprocess(&out_tensor)?;
            blender1.blend_tile(tile, &out_tile, plan1.overlap);
        }

        let pass1_img = blender1.finalize();
        cancel.check()?;

        // --- PASS 2: 2x Super-Resolution (giving 4x * 2x = 8x total) ---
        let (w2, h2) = pass1_img.dimensions();
        let plan2 = TilePlan::build(w2, h2, 32, 8);
        let mut session2 = self.engine.load(b"pass2_bytes", None)?;
        let mut blender2 = TileBlender::new(w2, h2, 2);

        for tile in &plan2.tiles {
            cancel.check()?;
            let mut tile_img = RgbImage::new(tile.width, tile.height);
            for ty in 0..tile.height {
                for tx in 0..tile.width {
                    tile_img.put_pixel(tx, ty, *pass1_img.get_pixel(tile.x + tx, tile.y + ty));
                }
            }

            let in_tensor = adapter_pass2.preprocess(&tile_img)?;
            let out_tensor = self.engine.run(&mut *session2, in_tensor.view(), cancel)?;
            let out_tile = adapter_pass2.postprocess(&out_tensor)?;
            blender2.blend_tile(tile, &out_tile, plan2.overlap);
        }

        let mut final_8x_img = blender2.finalize();
        cancel.check()?;

        // If target scale is not 8 (e.g. 6x requested via 8x cascade), downsample via Lanczos3
        if target_scale != 8 {
            let tw = (w1 * target_scale).max(1);
            let th = (h1 * target_scale).max(1);
            final_8x_img = downsample_lanczos3(&final_8x_img, tw, th)?;
        }

        Ok(final_8x_img)
    }
}

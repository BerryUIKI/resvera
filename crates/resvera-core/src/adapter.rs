use crate::engine::{EngineError, OwnedTensor};
use image::RgbImage;
use resvera_models::ModelManifest;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("Engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("Adapter validation error: {0}")]
    Validation(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Dimension mismatch: {0}")]
    DimensionMismatch(String),
    #[error("Cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileConstraints {
    pub alignment: u32,
    pub minimum: u32,
    pub recommended: u32,
    pub overlap: u32,
    pub window_size: Option<u32>,
    pub static_shapes_required: bool,
}

pub trait ModelAdapter: Send + Sync {
    fn family(&self) -> &'static str;
    fn validate_manifest(&self, manifest: &ModelManifest) -> Result<(), PipelineError>;
    fn tile_constraints(&self, manifest: &ModelManifest) -> TileConstraints;
    fn preprocess(&self, tile: &RgbImage) -> Result<OwnedTensor, PipelineError>;
    fn postprocess(&self, output: &OwnedTensor) -> Result<RgbImage, PipelineError>;
}

pub struct RrdbAdapter;

impl ModelAdapter for RrdbAdapter {
    fn family(&self) -> &'static str {
        "rrdb"
    }

    fn validate_manifest(&self, manifest: &ModelManifest) -> Result<(), PipelineError> {
        if manifest.family != "rrdb" && manifest.family != "rrdb-6b" {
            return Err(PipelineError::Validation(format!(
                "Unsupported family for RrdbAdapter: {}",
                manifest.family
            )));
        }
        if manifest.tensor.layout != "NCHW" || manifest.tensor.channels != "RGB" {
            return Err(PipelineError::Validation(
                "RrdbAdapter requires NCHW RGB layout".into(),
            ));
        }
        Ok(())
    }

    fn tile_constraints(&self, manifest: &ModelManifest) -> TileConstraints {
        TileConstraints {
            alignment: manifest.tiling.alignment.max(1),
            minimum: manifest.tiling.minimum.max(32),
            recommended: manifest.tiling.recommended.max(256),
            overlap: manifest.tiling.overlap.max(8),
            window_size: manifest.tiling.window_size,
            static_shapes_required: manifest.tiling.static_shapes_required,
        }
    }

    fn preprocess(&self, tile: &RgbImage) -> Result<OwnedTensor, PipelineError> {
        let (width, height) = tile.dimensions();
        let w = width as usize;
        let h = height as usize;
        let plane_size = w * h;

        let mut data = vec![0.0f32; 3 * plane_size];

        for y in 0..h {
            for x in 0..w {
                let pixel = tile.get_pixel(x as u32, y as u32);
                let idx = y * w + x;
                // Normalize uint8 [0, 255] to float32 [0.0, 1.0] in NCHW order
                data[idx] = pixel[0] as f32 / 255.0; // R
                data[plane_size + idx] = pixel[1] as f32 / 255.0; // G
                data[2 * plane_size + idx] = pixel[2] as f32 / 255.0; // B
            }
        }

        let tensor = OwnedTensor::new([1, 3, h, w], data)?;
        Ok(tensor)
    }

    fn postprocess(&self, output: &OwnedTensor) -> Result<RgbImage, PipelineError> {
        if output.shape[0] != 1 || output.shape[1] != 3 {
            return Err(PipelineError::DimensionMismatch(format!(
                "Expected shape [1, 3, H, W], got {:?}",
                output.shape
            )));
        }

        let h = output.shape[2];
        let w = output.shape[3];
        let plane_size = w * h;

        let mut img = RgbImage::new(w as u32, h as u32);

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let r = (output.data[idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                let g = (output.data[plane_size + idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                let b = (output.data[2 * plane_size + idx].clamp(0.0, 1.0) * 255.0).round() as u8;

                img.put_pixel(x as u32, y as u32, image::Rgb([r, g, b]));
            }
        }

        Ok(img)
    }
}

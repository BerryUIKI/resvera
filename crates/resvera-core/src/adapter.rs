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

pub struct CuganAdapter {
    pub scale: u32,
    pub pad: u32,
}

impl Default for CuganAdapter {
    fn default() -> Self {
        Self {
            scale: 2,
            pad: 18,
        }
    }
}

impl CuganAdapter {
    pub fn new(scale: u32) -> Self {
        Self {
            scale,
            pad: 18,
        }
    }

    fn reflect_coord(coord: i32, max_len: i32) -> u32 {
        if max_len <= 1 {
            return 0;
        }
        let mut c = coord;
        while c < 0 || c >= max_len {
            if c < 0 {
                c = -c;
            } else if c >= max_len {
                c = 2 * max_len - 2 - c;
            }
        }
        c as u32
    }
}

impl ModelAdapter for CuganAdapter {
    fn family(&self) -> &'static str {
        "cugan"
    }

    fn validate_manifest(&self, manifest: &ModelManifest) -> Result<(), PipelineError> {
        if manifest.family != "cugan" && manifest.family != "real-cugan" {
            return Err(PipelineError::Validation(format!(
                "Unsupported family for CuganAdapter: {}",
                manifest.family
            )));
        }
        if manifest.tensor.layout != "NCHW" || manifest.tensor.channels != "RGB" {
            return Err(PipelineError::Validation(
                "CuganAdapter requires NCHW RGB layout".into(),
            ));
        }
        Ok(())
    }

    fn tile_constraints(&self, manifest: &ModelManifest) -> TileConstraints {
        TileConstraints {
            alignment: manifest.tiling.alignment.max(2),
            minimum: manifest.tiling.minimum.max(64),
            recommended: manifest.tiling.recommended.max(256),
            overlap: manifest.tiling.overlap.max(18),
            window_size: manifest.tiling.window_size,
            static_shapes_required: manifest.tiling.static_shapes_required,
        }
    }

    fn preprocess(&self, tile: &RgbImage) -> Result<OwnedTensor, PipelineError> {
        let (width, height) = tile.dimensions();
        let w = width as i32;
        let h = height as i32;
        let pad = self.pad as i32;

        let padded_w = (w + 2 * pad) as usize;
        let padded_h = (h + 2 * pad) as usize;
        let plane_size = padded_w * padded_h;

        let mut data = vec![0.0f32; 3 * plane_size];

        for py in 0..padded_h {
            let sy = Self::reflect_coord((py as i32) - pad, h);
            for px in 0..padded_w {
                let sx = Self::reflect_coord((px as i32) - pad, w);
                let pixel = tile.get_pixel(sx, sy);
                let idx = py * padded_w + px;

                data[idx] = pixel[0] as f32 / 255.0;
                data[plane_size + idx] = pixel[1] as f32 / 255.0;
                data[2 * plane_size + idx] = pixel[2] as f32 / 255.0;
            }
        }

        OwnedTensor::new([1, 3, padded_h, padded_w], data).map_err(PipelineError::Engine)
    }

    fn postprocess(&self, output: &OwnedTensor) -> Result<RgbImage, PipelineError> {
        if output.shape[0] != 1 || output.shape[1] != 3 {
            return Err(PipelineError::DimensionMismatch(format!(
                "Expected shape [1, 3, H, W], got {:?}",
                output.shape
            )));
        }

        let padded_out_h = output.shape[2];
        let padded_out_w = output.shape[3];
        let pad_out = (self.pad * self.scale) as usize;

        if padded_out_h <= 2 * pad_out || padded_out_w <= 2 * pad_out {
            return Err(PipelineError::DimensionMismatch(format!(
                "Padded dimensions {:?} smaller than crop margins {}",
                output.shape,
                2 * pad_out
            )));
        }

        let out_h = padded_out_h - 2 * pad_out;
        let out_w = padded_out_w - 2 * pad_out;
        let plane_size = padded_out_w * padded_out_h;

        let mut img = RgbImage::new(out_w as u32, out_h as u32);

        for y in 0..out_h {
            let py = y + pad_out;
            for x in 0..out_w {
                let px = x + pad_out;
                let idx = py * padded_out_w + px;

                let r = (output.data[idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                let g = (output.data[plane_size + idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                let b = (output.data[2 * plane_size + idx].clamp(0.0, 1.0) * 255.0).round() as u8;

                img.put_pixel(x as u32, y as u32, image::Rgb([r, g, b]));
            }
        }

        Ok(img)
    }
}


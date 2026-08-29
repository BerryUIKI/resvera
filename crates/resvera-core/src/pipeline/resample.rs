use crate::adapter::PipelineError;
use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, Resizer};
use image::RgbImage;

/// Performs high-quality Lanczos3 resampling to target dimensions.
pub fn downsample_lanczos3(
    src: &RgbImage,
    target_width: u32,
    target_height: u32,
) -> Result<RgbImage, PipelineError> {
    if src.width() == target_width && src.height() == target_height {
        return Ok(src.clone());
    }

    let src_image = Image::from_vec_u8(
        src.width(),
        src.height(),
        src.as_raw().clone(),
        PixelType::U8x3,
    )
    .map_err(|e| PipelineError::DimensionMismatch(e.to_string()))?;

    let mut dst_image = Image::new(target_width, target_height, PixelType::U8x3);

    let mut resizer = Resizer::new();
    let resize_alg = ResizeAlg::Convolution(FilterType::Lanczos3);

    resizer
        .resize(&src_image, &mut dst_image, &fast_image_resize::ResizeOptions::new().resize_alg(resize_alg))
        .map_err(|e| PipelineError::DimensionMismatch(e.to_string()))?;

    let dst_raw = dst_image.into_vec();
    let dst_img = RgbImage::from_raw(target_width, target_height, dst_raw)
        .ok_or_else(|| PipelineError::DimensionMismatch("Failed to construct RgbImage from resized buffer".into()))?;

    Ok(dst_img)
}

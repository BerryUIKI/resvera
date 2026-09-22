use crate::adapter::PipelineError;
use crate::pipeline::metadata::{MetadataPolicy, RawImageMetadata};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{
    ExtendedColorType, GrayImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, RgbImage,
    RgbaImage,
};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutputFormat {
    SameAsInput,
    Png,
    Jpeg { quality: u8 },
    Webp { lossless: bool, quality: Option<u8> },
}

#[derive(Debug, Clone)]
pub struct LoadedImage {
    pub rgb: RgbImage,
    pub alpha: Option<GrayImage>,
    pub metadata: RawImageMetadata,
}

pub fn load_image<P: AsRef<Path>>(path: P) -> Result<RgbImage, PipelineError> {
    let loaded = load_image_with_alpha(path)?;
    Ok(loaded.rgb)
}

pub fn load_image_with_alpha<P: AsRef<Path>>(path: P) -> Result<LoadedImage, PipelineError> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let orientation_was_applied = orientation != image::metadata::Orientation::NoTransforms;

    let icc_profile = decoder.icc_profile().ok().flatten();
    let exif = decoder.exif_metadata().ok().flatten();
    let xmp = decoder.xmp_metadata().ok().flatten();

    let mut dyn_img = image::DynamicImage::from_decoder(decoder)?;
    dyn_img.apply_orientation(orientation);

    let alpha = if dyn_img.color().has_alpha() {
        let rgba = dyn_img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut alpha_buf = GrayImage::new(w, h);
        let mut has_non_opaque = false;
        for y in 0..h {
            for x in 0..w {
                let a = rgba.get_pixel(x, y)[3];
                if a < 255 {
                    has_non_opaque = true;
                }
                alpha_buf.put_pixel(x, y, image::Luma([a]));
            }
        }
        if has_non_opaque {
            Some(alpha_buf)
        } else {
            None
        }
    } else {
        None
    };

    let rgb = dyn_img.to_rgb8();
    let metadata = RawImageMetadata {
        icc_profile,
        exif,
        xmp,
        orientation_was_applied,
    };
    Ok(LoadedImage {
        rgb,
        alpha,
        metadata,
    })
}

pub fn save_image<P: AsRef<Path>>(
    img: &RgbImage,
    path: P,
    format: &OutputFormat,
    original_input_path: Option<&Path>,
) -> Result<(), PipelineError> {
    save_image_with_alpha(img, None, path, format, original_input_path)
}

pub fn save_image_with_alpha<P: AsRef<Path>>(
    rgb: &RgbImage,
    alpha: Option<&GrayImage>,
    path: P,
    format: &OutputFormat,
    original_input_path: Option<&Path>,
) -> Result<(), PipelineError> {
    save_image_with_metadata(
        rgb,
        alpha,
        path,
        format,
        original_input_path,
        None,
        &MetadataPolicy::default(),
    )
}

pub fn save_image_with_metadata<P: AsRef<Path>>(
    rgb: &RgbImage,
    alpha: Option<&GrayImage>,
    path: P,
    format: &OutputFormat,
    original_input_path: Option<&Path>,
    metadata: Option<&RawImageMetadata>,
    policy: &MetadataPolicy,
) -> Result<(), PipelineError> {
    let path = path.as_ref();
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    let effective_format = match format {
        OutputFormat::SameAsInput => {
            if let Some(orig) = original_input_path {
                match orig
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_lowercase())
                    .as_deref()
                {
                    Some("jpg") | Some("jpeg") => ImageFormat::Jpeg,
                    Some("webp") => ImageFormat::WebP,
                    _ => ImageFormat::Png,
                }
            } else {
                ImageFormat::Png
            }
        }
        OutputFormat::Png => ImageFormat::Png,
        OutputFormat::Jpeg { .. } => ImageFormat::Jpeg,
        OutputFormat::Webp { .. } => ImageFormat::WebP,
    };

    let (w, h) = rgb.dimensions();

    // If alpha is provided and dimensions match, build RGBA buffer
    let rgba_opt = match alpha {
        Some(a) if a.dimensions() == (w, h) => {
            let mut rgba = RgbaImage::new(w, h);
            for y in 0..h {
                for x in 0..x_max(w) {
                    let rgb_p = rgb.get_pixel(x, y);
                    let a_p = a.get_pixel(x, y)[0];
                    rgba.put_pixel(x, y, image::Rgba([rgb_p[0], rgb_p[1], rgb_p[2], a_p]));
                }
            }
            Some(rgba)
        }
        _ => None,
    };

    let sanitized = metadata.map(|m| m.sanitize_for_output(policy));

    match effective_format {
        ImageFormat::Png => {
            let mut encoder = PngEncoder::new(&mut writer);
            if let Some(ref meta) = sanitized {
                if let Some(ref icc) = meta.icc_profile {
                    let _ = encoder.set_icc_profile(icc.clone());
                }
                if let Some(ref exif) = meta.exif {
                    let _ = encoder.set_exif_metadata(exif.clone());
                }
            }

            if let Some(ref rgba) = rgba_opt {
                encoder.write_image(rgba.as_raw(), w, h, ExtendedColorType::Rgba8)?;
            } else {
                encoder.write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)?;
            }
        }
        ImageFormat::Jpeg => {
            let quality = match format {
                OutputFormat::Jpeg { quality } => *quality,
                _ => 90,
            };
            let mut encoder = JpegEncoder::new_with_quality(&mut writer, quality);
            if let Some(ref meta) = sanitized {
                if let Some(ref icc) = meta.icc_profile {
                    let _ = encoder.set_icc_profile(icc.clone());
                }
                if let Some(ref exif) = meta.exif {
                    let _ = encoder.set_exif_metadata(exif.clone());
                }
            }
            encoder.write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)?;
        }
        ImageFormat::WebP => {
            if let OutputFormat::Webp {
                lossless: false, ..
            } = format
            {
                return Err(PipelineError::Validation(
                    "Lossy WebP encoding is not supported by the pure-Rust encoder; please select lossless WebP, PNG, or JPEG.".into(),
                ));
            }
            let encoder = WebPEncoder::new_lossless(&mut writer);
            if let Some(ref rgba) = rgba_opt {
                encoder.encode(rgba.as_raw(), w, h, ExtendedColorType::Rgba8)?;
            } else {
                encoder.encode(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)?;
            }
        }
        _ => {
            if let Some(ref rgba) = rgba_opt {
                rgba.write_to(&mut writer, ImageFormat::Png)?;
            } else {
                rgb.write_to(&mut writer, ImageFormat::Png)?;
            }
        }
    }

    Ok(())
}

#[inline]
fn x_max(w: u32) -> u32 {
    w
}

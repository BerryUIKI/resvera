use crate::adapter::PipelineError;
use image::{ImageFormat, ImageReader, RgbImage};
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

pub fn load_image<P: AsRef<Path>>(path: P) -> Result<RgbImage, PipelineError> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let dyn_img = reader.decode()?;
    Ok(dyn_img.to_rgb8())
}

pub fn save_image<P: AsRef<Path>>(
    img: &RgbImage,
    path: P,
    format: &OutputFormat,
    original_input_path: Option<&Path>,
) -> Result<(), PipelineError> {
    let path = path.as_ref();
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    let effective_format = match format {
        OutputFormat::SameAsInput => {
            if let Some(orig) = original_input_path {
                match orig.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref() {
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

    match effective_format {
        ImageFormat::Png => {
            img.write_to(&mut writer, ImageFormat::Png)?;
        }
        ImageFormat::Jpeg => {
            let quality = match format {
                OutputFormat::Jpeg { quality } => *quality,
                _ => 90,
            };
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality);
            encoder.encode(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)?;
        }
        ImageFormat::WebP => {
            img.write_to(&mut writer, ImageFormat::WebP)?;
        }
        _ => {
            img.write_to(&mut writer, ImageFormat::Png)?;
        }
    }

    Ok(())
}

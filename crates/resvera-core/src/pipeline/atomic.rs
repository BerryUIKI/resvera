use crate::adapter::PipelineError;
use crate::pipeline::io::{save_image, OutputFormat};
use image::RgbImage;
use std::fs;
use std::path::{Path, PathBuf};

pub fn generate_output_path(
    output_dir: &Path,
    input_path: &Path,
    model_id: &str,
    target_scale: u32,
    format: &OutputFormat,
    overwrite: bool,
) -> PathBuf {
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");

    let ext = match format {
        OutputFormat::SameAsInput => input_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png"),
        OutputFormat::Png => "png",
        OutputFormat::Jpeg { .. } => "jpg",
        OutputFormat::Webp { .. } => "webp",
    };

    let base_name = format!("{}_{}_{}x.{}", stem, model_id, target_scale, ext);
    let initial_path = output_dir.join(&base_name);

    if overwrite || !initial_path.exists() {
        return initial_path;
    }

    let mut counter = 1;
    loop {
        let candidate_name = format!("{}_{}_{}x_{}.{}", stem, model_id, target_scale, counter, ext);
        let candidate_path = output_dir.join(&candidate_name);
        if !candidate_path.exists() {
            return candidate_path;
        }
        counter += 1;
    }
}

pub fn atomic_save_image(
    img: &RgbImage,
    target_path: &Path,
    format: &OutputFormat,
    original_input_path: Option<&Path>,
) -> Result<(), PipelineError> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = target_path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
    
    // Save to temp file
    save_image(img, &tmp_path, format, original_input_path)?;

    // Atomically rename
    if let Err(e) = fs::rename(&tmp_path, target_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(PipelineError::Io(e));
    }

    Ok(())
}

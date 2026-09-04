use crate::adapter::PipelineError;
use crate::pipeline::io::{save_image, OutputFormat};
use crate::pipeline::naming::sanitize_filename_component;
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
    let raw_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let stem = sanitize_filename_component(raw_stem);
    let model_safe = sanitize_filename_component(model_id);

    let ext = match format {
        OutputFormat::SameAsInput => input_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png"),
        OutputFormat::Png => "png",
        OutputFormat::Jpeg { .. } => "jpg",
        OutputFormat::Webp { .. } => "webp",
    };
    let safe_ext = sanitize_filename_component(ext.trim_start_matches('.'));

    let base_name = format!("{}_{}_{}x.{}", stem, model_safe, target_scale, safe_ext);
    let initial_path = output_dir.join(&base_name);

    if overwrite || !initial_path.exists() {
        return initial_path;
    }

    let mut counter = 1;
    loop {
        let candidate_name = format!(
            "{}_{}_{}x_{}.{}",
            stem, model_safe, target_scale, counter, safe_ext
        );
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

    // Save to a sibling temporary file so the final rename stays on one filesystem.
    if let Err(error) = save_image(img, &tmp_path, format, original_input_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }

    if target_path.exists() {
        let target_metadata = fs::symlink_metadata(target_path)?;
        if !target_metadata.file_type().is_file() {
            let _ = fs::remove_file(&tmp_path);
            return Err(PipelineError::Validation(format!(
                "Output target is not a regular file: {}",
                target_path.display()
            )));
        }

        // Windows cannot rename over an existing file. Move the prior output aside and restore
        // it if committing the new file fails.
        let backup_path = target_path.with_extension(format!("backup.{}", uuid::Uuid::new_v4()));
        if let Err(error) = fs::rename(target_path, &backup_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(PipelineError::Io(error));
        }
        if let Err(error) = fs::rename(&tmp_path, target_path) {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::rename(&backup_path, target_path);
            return Err(PipelineError::Io(error));
        }
        let _ = fs::remove_file(backup_path);
    } else if let Err(error) = fs::rename(&tmp_path, target_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(PipelineError::Io(error));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use tempfile::tempdir;

    #[test]
    fn replaces_existing_outputs_without_leaving_transaction_files() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("output.png");
        fs::write(&target, b"old output").unwrap();
        let image = RgbImage::from_pixel(2, 2, Rgb([1, 2, 3]));

        atomic_save_image(&image, &target, &OutputFormat::Png, None).unwrap();

        assert!(image::open(&target).is_ok());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn refuses_to_replace_non_file_targets() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("output.png");
        fs::create_dir(&target).unwrap();
        let image = RgbImage::from_pixel(1, 1, Rgb([1, 2, 3]));

        assert!(atomic_save_image(&image, &target, &OutputFormat::Png, None).is_err());
        assert!(target.is_dir());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

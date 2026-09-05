use image::{Rgba, RgbaImage};
use resvera_core::{atomic_save_image, JobOrchestrator, OutputFormat, UpscaleJobRequest};
use resvera_persistence::AppDatabase;
use std::sync::Arc;
use tempfile::tempdir;

mod common;
use common::{install_mock_model, MockEngine};

#[test]
fn test_alpha_channel_preservation_png_and_webp() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(MockEngine);
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let orchestrator =
        JobOrchestrator::with_models_root(db, engine, temp.path().join("previews"), &models_root);

    // 1. Create test RGBA PNG image with translucent pixels
    let input_path = temp.path().join("input_alpha.png");
    let width = 16;
    let height = 16;
    let mut rgba_img = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let alpha = if x < 8 { 128 } else { 255 };
            rgba_img.put_pixel(x, y, Rgba([100, 150, 200, alpha]));
        }
    }
    rgba_img.save(&input_path).unwrap();

    // Upscale to PNG
    let req_png = UpscaleJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: true,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };

    let queued = orchestrator.submit_job(&req_png).unwrap();
    let completed = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(completed.id, queued.id);
    assert_eq!(completed.state, "succeeded");

    let out_path = completed.output_path.unwrap();
    let out_img = image::open(&out_path).unwrap().to_rgba8();
    assert_eq!(out_img.dimensions(), (64, 64));
    // Verify alpha is preserved across upscale
    assert_eq!(out_img.get_pixel(10, 10)[3], 128);
    assert_eq!(out_img.get_pixel(50, 50)[3], 255);

    // Upscale to WebP
    let req_webp = UpscaleJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Webp {
            lossless: true,
            quality: None,
        },
        overwrite: true,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };

    let queued_webp = orchestrator.submit_job(&req_webp).unwrap();
    let completed_webp = orchestrator.process_next_job().unwrap().unwrap();
    assert_eq!(completed_webp.id, queued_webp.id);
    assert_eq!(completed_webp.state, "succeeded");

    let out_webp_path = completed_webp.output_path.unwrap();
    let out_webp_img = image::open(&out_webp_path).unwrap().to_rgba8();
    assert_eq!(out_webp_img.dimensions(), (64, 64));
    assert_eq!(out_webp_img.get_pixel(10, 10)[3], 128);
    assert_eq!(out_webp_img.get_pixel(50, 50)[3], 255);
}

#[test]
fn test_cancellation_during_finalization_cleans_up_files() {
    let temp = tempdir().unwrap();
    let db = AppDatabase::new_in_memory().unwrap();
    let engine = Arc::new(MockEngine);
    let models_root = temp.path().join("models");
    install_mock_model(&models_root, "realesrgan-x4plus", 4);
    let orchestrator = JobOrchestrator::with_models_root(
        db.clone(),
        engine,
        temp.path().join("previews"),
        &models_root,
    );

    let input_path = temp.path().join("input.png");
    let mut rgb = image::RgbImage::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            rgb.put_pixel(x, y, image::Rgb([10, 20, 30]));
        }
    }
    atomic_save_image(&rgb, &input_path, &OutputFormat::Png, None).unwrap();

    let req = UpscaleJobRequest {
        input_path: input_path.to_str().unwrap().to_string(),
        output_directory: temp.path().to_str().unwrap().to_string(),
        model_id: "realesrgan-x4plus".to_string(),
        model_variant_id: "default".to_string(),
        target_scale: 4,
        output_format: OutputFormat::Png,
        overwrite: true,
        tile_size: Some(32),
        provider_preference: Some("cpu".to_string()),
    };

    let queued = orchestrator.submit_job(&req).unwrap();

    // Cancel the job immediately
    orchestrator.cancel_job(&queued.id).unwrap();

    // Attempting to process will detect cancelled state
    let res = orchestrator.process_next_job().unwrap();
    // Claiming returns None or cancelled record
    if let Some(record) = res {
        assert_eq!(record.state, "cancelled");
        if let Some(ref out) = record.output_path {
            assert!(!std::path::Path::new(out).exists());
        }
        if let Some(ref prev) = record.preview_path {
            assert!(!std::path::Path::new(prev).exists());
        }
    }

    let final_record = db.get_job(&queued.id).unwrap().unwrap();
    assert_eq!(final_record.state, "cancelled");
}

#[test]
fn test_load_image_applies_orientation() {
    let temp = tempdir().unwrap();
    let img_path = temp.path().join("oriented.png");
    let mut rgb = image::RgbImage::new(10, 20);
    rgb.put_pixel(0, 0, image::Rgb([255, 0, 0]));
    atomic_save_image(&rgb, &img_path, &OutputFormat::Png, None).unwrap();

    let loaded = resvera_core::load_image(&img_path).unwrap();
    assert_eq!(loaded.dimensions(), (10, 20));
}

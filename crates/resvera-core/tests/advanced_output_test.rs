use image::{Rgb, RgbImage};
use resvera_core::{
    format_output_filename, CancellationToken, CascadePipeline, CuganAdapter, MetadataPolicy,
    RrdbAdapter, SanitizedMetadata,
};
use std::sync::Arc;

#[test]
fn test_metadata_sanitization_policy() {
    let mut meta = SanitizedMetadata {
        camera_make: Some("Sony".into()),
        camera_model: Some("A7 IV".into()),
        date_time: Some("2026:08:29 12:00:00".into()),
        color_space: Some("sRGB".into()),
        gps_latitude: Some(37.7749),
        gps_longitude: Some(-122.4194),
        width: 1920,
        height: 1080,
    };

    // 1. Preserve safe without GPS -> GPS stripped, camera preserved
    meta.apply_policy(&MetadataPolicy::PreserveSafe {
        preserve_gps: false,
    });
    assert_eq!(meta.camera_make, Some("Sony".into()));
    assert_eq!(meta.gps_latitude, None);
    assert_eq!(meta.gps_longitude, None);

    // 2. Strip all -> everything stripped
    meta.apply_policy(&MetadataPolicy::Strip);
    assert_eq!(meta.camera_make, None);
    assert_eq!(meta.date_time, None);
}

#[test]
fn test_output_filename_templating() {
    let filename = format_output_filename(
        "{stem}_{model}_{scale}x_custom",
        "landscape",
        "realesrgan",
        4,
        "png",
    );
    assert_eq!(filename, "landscape_realesrgan_4x_custom.png");

    let default_fmt = format_output_filename("", "portrait", "cugan", 2, "jpg");
    assert_eq!(default_fmt, "portrait_cugan_2x.jpg");
}

#[test]
fn test_8x_cascade_upscale_pipeline() {
    let engine = Arc::new(MockEngine);
    let cascade = CascadePipeline::new(engine);

    let mut img = RgbImage::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            img.put_pixel(x, y, Rgb([50, 100, 150]));
        }
    }

    let adapter1 = RrdbAdapter;
    let adapter2 = CuganAdapter::new(2);
    let cancel = CancellationToken::new();

    // Run 8x cascade (16x16 -> 128x128)
    let out = cascade
        .run_8x_cascade_with_weights(
            &img,
            &adapter1,
            b"verified-4x-model",
            &adapter2,
            b"verified-2x-model",
            8,
            &cancel,
        )
        .unwrap();
    assert_eq!(out.dimensions(), (128, 128));

    // Test cancellation
    cancel.cancel();
    let err = cascade.run_8x_cascade_with_weights(
        &img,
        &adapter1,
        b"verified-4x-model",
        &adapter2,
        b"verified-2x-model",
        8,
        &cancel,
    );
    assert!(err.is_err());
}
mod common;

use common::MockEngine;

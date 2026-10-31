use image::{Rgb, RgbImage};
use resvera_core::{
    format_output_filename, BlendMode, CancellationToken, CascadePass, CascadePipeline,
    MetadataPolicy, NoProgress, RrdbAdapter, SanitizedMetadata, TileConstraints,
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

    // Both shipped catalog models are 4x, so an 8x job is 4x + 4x = 16x followed
    // by a Lanczos3 downsample to 8x.
    let adapter = RrdbAdapter;
    let constraints = rrdb_constraints();
    let passes = [
        CascadePass {
            adapter: &adapter,
            weights: b"verified-4x-model",
            constraints,
            native_scale: 4,
        },
        CascadePass {
            adapter: &adapter,
            weights: b"verified-4x-model",
            constraints,
            native_scale: 4,
        },
    ];

    let cancel = CancellationToken::new();
    let out = cascade
        .run(
            &img,
            &passes,
            8,
            BlendMode::Cosine,
            &cancel,
            &mut NoProgress,
        )
        .unwrap();
    assert_eq!(out.dimensions(), (128, 128));

    // Cancellation is honoured before work starts.
    cancel.cancel();
    let err = cascade.run(
        &img,
        &passes,
        8,
        BlendMode::Cosine,
        &cancel,
        &mut NoProgress,
    );
    assert!(err.is_err());
}

#[test]
fn test_cascade_total_scale_and_dimensions() {
    let adapter = RrdbAdapter;
    let constraints = rrdb_constraints();
    let passes = [
        CascadePass {
            adapter: &adapter,
            weights: b"w",
            constraints,
            native_scale: 4,
        },
        CascadePass {
            adapter: &adapter,
            weights: b"w",
            constraints,
            native_scale: 4,
        },
    ];
    assert_eq!(CascadePipeline::total_scale(&passes), 16);
    assert_eq!(
        CascadePipeline::output_dimensions(&passes, 10, 20).unwrap(),
        (160, 320)
    );
}

#[test]
fn test_cascade_rejects_oversized_working_buffer() {
    // The peak buffer for a 4x + 4x chain is the 16x intermediate, not the
    // requested 8x output, so this must fail before allocating anything.
    let engine = Arc::new(MockEngine);
    let cascade = CascadePipeline::new(engine);
    let adapter = RrdbAdapter;
    let constraints = rrdb_constraints();
    let passes = [
        CascadePass {
            adapter: &adapter,
            weights: b"w",
            constraints,
            native_scale: 4,
        },
        CascadePass {
            adapter: &adapter,
            weights: b"w",
            constraints,
            native_scale: 4,
        },
    ];

    // 1000x1000 at 16x is 256 MP, far past the blend-buffer ceiling.
    let img = RgbImage::new(1000, 1000);
    let err = cascade
        .run(
            &img,
            &passes,
            8,
            BlendMode::Cosine,
            &CancellationToken::new(),
            &mut NoProgress,
        )
        .expect_err("oversized cascade must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("blending limit"),
        "error should name the limit, got: {message}"
    );
}

/// Constraints matching the Real-ESRGAN manifests in the production catalog.
fn rrdb_constraints() -> TileConstraints {
    TileConstraints {
        alignment: 1,
        minimum: 32,
        recommended: 256,
        overlap: 16,
        window_size: None,
        static_shapes_required: false,
    }
}

mod common;

use common::MockEngine;

#[test]
fn test_cascade_uses_model_tile_constraints_not_hardcoded_values() {
    // The previous implementation hardcoded tile size 32 and overlap 8 for every
    // model. A model whose minimum exceeds 32 must still run correctly, which
    // only holds if the pass derives its tiling from the manifest constraints.
    let engine = Arc::new(MockEngine);
    let cascade = CascadePipeline::new(engine);

    let mut img = RgbImage::new(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            img.put_pixel(x, y, Rgb([x as u8, y as u8, 64]));
        }
    }

    let adapter = RrdbAdapter;
    // Mirrors CuganAdapter's constraints: minimum 64 and overlap 18, both above
    // the values the old implementation hardcoded.
    let constraints = TileConstraints {
        alignment: 2,
        minimum: 40,
        recommended: 64,
        overlap: 18,
        window_size: None,
        static_shapes_required: false,
    };
    let passes = [CascadePass {
        adapter: &adapter,
        weights: b"verified-4x-model",
        constraints,
        native_scale: 4,
    }];

    let out = cascade
        .run(
            &img,
            &passes,
            4,
            BlendMode::Cosine,
            &CancellationToken::new(),
            &mut NoProgress,
        )
        .unwrap();
    assert_eq!(out.dimensions(), (128, 128));
}

#[test]
fn test_cascade_rejects_overlap_larger_than_tile() {
    let engine = Arc::new(MockEngine);
    let cascade = CascadePipeline::new(engine);
    let img = RgbImage::new(16, 16);
    let adapter = RrdbAdapter;
    // recommended rounds up from minimum, so an overlap at or above the tile
    // size is a manifest inconsistency and must be reported, not looped on.
    let constraints = TileConstraints {
        alignment: 1,
        minimum: 32,
        recommended: 32,
        overlap: 32,
        window_size: None,
        static_shapes_required: false,
    };
    let passes = [CascadePass {
        adapter: &adapter,
        weights: b"w",
        constraints,
        native_scale: 4,
    }];

    let err = cascade
        .run(
            &img,
            &passes,
            4,
            BlendMode::Cosine,
            &CancellationToken::new(),
            &mut NoProgress,
        )
        .expect_err("overlap >= tile size must be rejected");
    assert!(
        err.to_string().contains("overlap"),
        "unexpected error: {err}"
    );
}

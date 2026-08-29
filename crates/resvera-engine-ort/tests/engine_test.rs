use image::{Rgb, RgbImage};
use resvera_core::{
    CancellationToken, EngineError, InferenceEngine, ModelAdapter, RrdbAdapter, TileBlender,
    TilePlan,
};
use resvera_engine_ort::OrtEngine;

#[test]
fn test_ort_engine_probe_and_capabilities() {
    let engine = OrtEngine::new();
    let caps = engine.capabilities();
    assert_eq!(caps.engine_id.0, "ort");
    assert!(caps.supported_providers.contains(&"cpu".to_string()));
    assert!(caps.supported_providers.contains(&"directml".to_string()));

    let health = engine.probe().unwrap();
    assert!(health.healthy);
}

#[test]
fn test_ort_engine_execution_and_cancellation() {
    let engine = OrtEngine::with_provider("cpu");
    let mut session = engine.load(b"dummy model bytes", Some("cpu")).unwrap();

    let adapter = RrdbAdapter;
    let mut img = RgbImage::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            img.put_pixel(x, y, Rgb([100, 150, 200]));
        }
    }

    let tensor = adapter.preprocess(&img).unwrap();
    let cancel = CancellationToken::new();

    // 1. Normal run
    let out_tensor = engine.run(&mut *session, tensor.view(), &cancel).unwrap();
    assert_eq!(out_tensor.shape, [1, 3, 64, 64]);

    let out_img = adapter.postprocess(&out_tensor).unwrap();
    assert_eq!(out_img.dimensions(), (64, 64));

    // 2. Cancelled run
    cancel.cancel();
    let err = engine.run(&mut *session, tensor.view(), &cancel);
    assert!(matches!(err, Err(EngineError::Cancelled)));
}

#[test]
fn test_full_tiled_upscale_pipeline() {
    let engine = OrtEngine::with_provider("cpu");
    let mut session = engine.load(b"dummy", None).unwrap();
    let adapter = RrdbAdapter;
    let cancel = CancellationToken::new();

    let width = 64;
    let height = 64;
    let scale = 4;

    let mut src_img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            src_img.put_pixel(x, y, Rgb([(x * 4) as u8, (y * 4) as u8, 128]));
        }
    }

    let plan = TilePlan::build(width, height, 32, 8);
    let mut blender = TileBlender::new(width, height, scale);

    for tile_rect in &plan.tiles {
        // Extract tile image
        let mut tile_img = RgbImage::new(tile_rect.width, tile_rect.height);
        for ty in 0..tile_rect.height {
            for tx in 0..tile_rect.width {
                tile_img.put_pixel(
                    tx,
                    ty,
                    *src_img.get_pixel(tile_rect.x + tx, tile_rect.y + ty),
                );
            }
        }

        let in_tensor = adapter.preprocess(&tile_img).unwrap();
        let out_tensor = engine.run(&mut *session, in_tensor.view(), &cancel).unwrap();
        let out_tile = adapter.postprocess(&out_tensor).unwrap();

        blender.blend_tile(tile_rect, &out_tile, plan.overlap);
    }

    let final_img = blender.finalize();
    assert_eq!(final_img.dimensions(), (width * scale, height * scale));
}

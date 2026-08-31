use image::{Rgb, RgbImage};
use resvera_core::{
    atomic_save_image, downsample_lanczos3, generate_output_path, ModelAdapter, OutputFormat,
    RrdbAdapter, TileBlender, TilePlan,
};
use tempfile::tempdir;

#[test]
fn test_rrdb_adapter_preprocess_postprocess() {
    let adapter = RrdbAdapter;
    let mut img = RgbImage::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            img.put_pixel(x, y, Rgb([(x * 15) as u8, (y * 15) as u8, 128]));
        }
    }

    let tensor = adapter.preprocess(&img).unwrap();
    assert_eq!(tensor.shape, [1, 3, 16, 16]);

    let reconstructed = adapter.postprocess(&tensor).unwrap();
    assert_eq!(reconstructed.dimensions(), (16, 16));

    // Verify values match original
    for y in 0..16 {
        for x in 0..16 {
            let orig = img.get_pixel(x, y);
            let recon = reconstructed.get_pixel(x, y);
            assert_eq!(orig[0], recon[0]);
            assert_eq!(orig[1], recon[1]);
            assert_eq!(orig[2], recon[2]);
        }
    }
}

#[test]
fn test_tiling_and_seamless_blending() {
    let width = 64;
    let height = 64;
    let scale = 4;
    let mut img = RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            img.put_pixel(
                x,
                y,
                Rgb([(x * 3) as u8, (y * 3) as u8, ((x + y) * 2) as u8]),
            );
        }
    }

    let plan = TilePlan::build(width, height, 32, 8);
    let mut blender = TileBlender::try_new(width, height, scale).unwrap();

    for tile in &plan.tiles {
        // Create an exact 4x nearest upscale for testing the blender
        let mut out_tile = RgbImage::new(tile.width * scale, tile.height * scale);
        for ty in 0..out_tile.height() {
            for tx in 0..out_tile.width() {
                let sx = tile.x + (tx / scale);
                let sy = tile.y + (ty / scale);
                out_tile.put_pixel(tx, ty, *img.get_pixel(sx, sy));
            }
        }
        blender.blend_tile(tile, &out_tile, plan.overlap);
    }

    let result = blender.finalize();
    assert_eq!(result.dimensions(), (width * scale, height * scale));

    // Check pixel at center to verify no seam distortion
    let orig = img.get_pixel(32, 32);
    let res = result.get_pixel(32 * scale, 32 * scale);
    assert_eq!(orig[0], res[0]);
    assert_eq!(orig[1], res[1]);
    assert_eq!(orig[2], res[2]);
}

#[test]
fn test_lanczos3_downsample() {
    let mut src = RgbImage::new(100, 100);
    for y in 0..100 {
        for x in 0..100 {
            src.put_pixel(x, y, Rgb([200, 100, 50]));
        }
    }

    let downsampled = downsample_lanczos3(&src, 50, 50).unwrap();
    assert_eq!(downsampled.dimensions(), (50, 50));
    let p = downsampled.get_pixel(25, 25);
    assert_eq!(p[0], 200);
    assert_eq!(p[1], 100);
    assert_eq!(p[2], 50);
}

#[test]
fn test_collision_safe_naming_and_atomic_save() {
    let temp = tempdir().unwrap();
    let out_dir = temp.path();
    let input_path = std::path::PathBuf::from("/test/photo.jpg");

    let p1 = generate_output_path(
        out_dir,
        &input_path,
        "realesrgan",
        4,
        &OutputFormat::Png,
        false,
    );
    assert_eq!(p1.file_name().unwrap(), "photo_realesrgan_4x.png");

    let img = RgbImage::new(10, 10);
    atomic_save_image(&img, &p1, &OutputFormat::Png, None).unwrap();
    assert!(p1.exists());

    // Without overwrite, generating output path should produce _1 suffix
    let p2 = generate_output_path(
        out_dir,
        &input_path,
        "realesrgan",
        4,
        &OutputFormat::Png,
        false,
    );
    assert_eq!(p2.file_name().unwrap(), "photo_realesrgan_4x_1.png");

    // With overwrite, should produce original path
    let p3 = generate_output_path(
        out_dir,
        &input_path,
        "realesrgan",
        4,
        &OutputFormat::Png,
        true,
    );
    assert_eq!(p3.file_name().unwrap(), "photo_realesrgan_4x.png");
}

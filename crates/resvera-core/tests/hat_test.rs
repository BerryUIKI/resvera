use image::{Rgb, RgbImage};
use resvera_core::{HatAdapter, ModelAdapter, OwnedTensor};

#[test]
fn test_hat_adapter_window_alignment_and_postprocessing() {
    let adapter = HatAdapter::new(4, 16); // 4x scale, 16px window size

    // Input image of non-aligned size 50x50
    let mut img = RgbImage::new(50, 50);
    for y in 0..50 {
        for x in 0..50 {
            img.put_pixel(x, y, Rgb([(x * 5) as u8, (y * 5) as u8, 120]));
        }
    }

    let tensor = adapter.preprocess(&img).unwrap();
    // 50 is padded up to next multiple of 16 -> 64
    assert_eq!(tensor.shape, [1, 3, 64, 64]);

    // Simulate 4x scale output
    let out_h = 64 * 4;
    let out_w = 64 * 4;
    let mut out_data = vec![0.0f32; 3 * out_h * out_w];
    let plane_size = out_h * out_w;
    let in_plane = 64 * 64;

    for c in 0..3 {
        for oy in 0..out_h {
            let iy = oy / 4;
            for ox in 0..out_w {
                let ix = ox / 4;
                out_data[c * plane_size + oy * out_w + ox] =
                    tensor.data[c * in_plane + iy * 64 + ix];
            }
        }
    }

    let out_tensor = OwnedTensor::new([1, 3, out_h, out_w], out_data).unwrap();
    let res_img = adapter.postprocess(&out_tensor).unwrap();

    assert_eq!(res_img.dimensions(), (256, 256));
    let pixel = res_img.get_pixel(0, 0);
    assert_eq!(pixel[2], 120);
}

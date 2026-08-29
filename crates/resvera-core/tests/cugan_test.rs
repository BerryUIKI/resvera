use image::{Rgb, RgbImage};
use resvera_core::{CuganAdapter, ModelAdapter, OwnedTensor};

#[test]
fn test_cugan_adapter_reflection_padding_and_crop() {
    let adapter = CuganAdapter::new(2); // 2x scale, 18px padding
    let mut img = RgbImage::new(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            img.put_pixel(x, y, Rgb([(x * 7) as u8, (y * 7) as u8, 100]));
        }
    }

    let padded_tensor = adapter.preprocess(&img).unwrap();
    // 32 + 2*18 = 68
    assert_eq!(padded_tensor.shape, [1, 3, 68, 68]);

    // Simulate 2x nearest evaluation of padded tensor
    let in_h = 68;
    let in_w = 68;
    let out_h = in_h * 2;
    let out_w = in_w * 2;
    let mut out_data = vec![0.0f32; 3 * out_h * out_w];
    let plane_size = out_h * out_w;
    let in_plane_size = in_h * in_w;

    for c in 0..3 {
        for oy in 0..out_h {
            let iy = oy / 2;
            for ox in 0..out_w {
                let ix = ox / 2;
                out_data[c * plane_size + oy * out_w + ox] =
                    padded_tensor.data[c * in_plane_size + iy * in_w + ix];
            }
        }
    }

    let eval_tensor = OwnedTensor::new([1, 3, out_h, out_w], out_data).unwrap();
    let cropped_img = adapter.postprocess(&eval_tensor).unwrap();

    // Cropped dimensions must be exactly 32 * 2 = 64
    assert_eq!(cropped_img.dimensions(), (64, 64));

    // Verify center pixel matches original
    let orig = img.get_pixel(16, 16);
    let upscaled = cropped_img.get_pixel(32, 32);
    assert_eq!(orig[0], upscaled[0]);
    assert_eq!(orig[1], upscaled[1]);
    assert_eq!(orig[2], upscaled[2]);
}

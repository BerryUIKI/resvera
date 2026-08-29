use image::RgbImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct TilePlan {
    pub img_width: u32,
    pub img_height: u32,
    pub tile_size: u32,
    pub overlap: u32,
    pub tiles: Vec<TileRect>,
}

impl TilePlan {
    pub fn build(img_width: u32, img_height: u32, tile_size: u32, overlap: u32) -> Self {
        let mut tiles = Vec::new();
        let step = (tile_size - overlap).max(1);

        let mut y = 0;
        while y < img_height {
            let h = if y + tile_size > img_height {
                img_height - y
            } else {
                tile_size
            };

            let mut x = 0;
            while x < img_width {
                let w = if x + tile_size > img_width {
                    img_width - x
                } else {
                    tile_size
                };

                tiles.push(TileRect {
                    x,
                    y,
                    width: w,
                    height: h,
                });

                if x + tile_size >= img_width {
                    break;
                }
                x += step;
            }

            if y + tile_size >= img_height {
                break;
            }
            y += step;
        }

        Self {
            img_width,
            img_height,
            tile_size,
            overlap,
            tiles,
        }
    }
}

pub struct TileBlender {
    out_width: u32,
    out_height: u32,
    scale: u32,
    accum_r: Vec<f32>,
    accum_g: Vec<f32>,
    accum_b: Vec<f32>,
    weights: Vec<f32>,
}

impl TileBlender {
    pub fn new(img_width: u32, img_height: u32, scale: u32) -> Self {
        let out_width = img_width * scale;
        let out_height = img_height * scale;
        let size = (out_width * out_height) as usize;

        Self {
            out_width,
            out_height,
            scale,
            accum_r: vec![0.0; size],
            accum_g: vec![0.0; size],
            accum_b: vec![0.0; size],
            weights: vec![0.0; size],
        }
    }

    /// Blends an upscaled output tile into the composite canvas with 2D feathering weights.
    pub fn blend_tile(&mut self, input_rect: &TileRect, output_tile: &RgbImage, overlap: u32) {
        let out_x = input_rect.x * self.scale;
        let out_y = input_rect.y * self.scale;
        let (tile_w, tile_h) = output_tile.dimensions();
        let feather = (overlap * self.scale) as f32;

        for ty in 0..tile_h {
            let cy = out_y + ty;
            if cy >= self.out_height {
                continue;
            }

            // Calculate vertical feather weight
            let mut wy = 1.0f32;
            if out_y > 0 && ((ty as f32) < feather) {
                wy = wy.min(ty as f32 / feather);
            }
            if out_y + tile_h < self.out_height && (((tile_h - 1 - ty) as f32) < feather) {
                wy = wy.min((tile_h - 1 - ty) as f32 / feather);
            }

            for tx in 0..tile_w {
                let cx = out_x + tx;
                if cx >= self.out_width {
                    continue;
                }

                // Calculate horizontal feather weight
                let mut wx = 1.0f32;
                if out_x > 0 && ((tx as f32) < feather) {
                    wx = wx.min(tx as f32 / feather);
                }
                if out_x + tile_w < self.out_width && (((tile_w - 1 - tx) as f32) < feather) {
                    wx = wx.min((tile_w - 1 - tx) as f32 / feather);
                }

                let w = (wx * wy).max(1e-4);
                let pixel = output_tile.get_pixel(tx, ty);
                let idx = (cy * self.out_width + cx) as usize;

                self.accum_r[idx] += pixel[0] as f32 * w;
                self.accum_g[idx] += pixel[1] as f32 * w;
                self.accum_b[idx] += pixel[2] as f32 * w;
                self.weights[idx] += w;
            }
        }
    }

    /// Finalizes the blended canvas and returns the reconstructed image.
    pub fn finalize(self) -> RgbImage {
        let mut img = RgbImage::new(self.out_width, self.out_height);

        for y in 0..self.out_height {
            for x in 0..self.out_width {
                let idx = (y * self.out_width + x) as usize;
                let w = self.weights[idx].max(1e-6);

                let r = (self.accum_r[idx] / w).clamp(0.0, 255.0).round() as u8;
                let g = (self.accum_g[idx] / w).clamp(0.0, 255.0).round() as u8;
                let b = (self.accum_b[idx] / w).clamp(0.0, 255.0).round() as u8;

                img.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }

        img
    }
}

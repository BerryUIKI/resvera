# Resvera User Guide / 用户使用指南

Resvera is a cross-platform, pure-offline AI image super-resolution desktop application.
Resvera 是一款支持全平台、纯离线运行的高性能 AI 图像超分辨率桌面放大工具。

---

## 🚀 Quick Start / 快速上手

### 1. Launching the App / 启动程序
- **Desktop Window (开发调试桌面端)**:
  ```bash
  pnpm tauri dev
  ```
- **Web Preview (浏览器热重载预览)**:
  ```bash
  pnpm run dev
  ```

### 2. Adding Images to Queue / 添加图像至处理队列
1. Drag and drop PNG, JPEG, or WebP images into the window or click **"Add Images to Queue" (添加图片至队列)**.
2. Select your target **Scale Factor** (1x, 2x, 4x, 8x Cascade).
3. Select your preferred **Output Format** (PNG, JPEG, WebP).
4. The processing queue runs sequentially with realtime progress and cancellation support.

### 3. Comparing Results / 实时对比放大效果
- In the center viewer, use the **Split Comparison Slider (滑动对比条)** to inspect the sharp upscaled output against the original image.
- Use `+` / `-` buttons or the mouse wheel to zoom in up to 300%.

---

## 🧠 Supported Models / 支持的模型架构

| Model Name / 模型名称 | Architecture / 架构 | Scales / 支持倍率 | Ideal For / 推荐场景 |
|---|---|---|---|
| **Real-ESRGAN x4plus** | RRDB | 4x | Real-world photography, landscapes, portraits (真实摄影与风景) |
| **Real-ESRGAN x4plus Anime (6B)** | RRDB-6B | 4x | Anime, digital illustration, line art (二次元插画与动漫) |
| **Real-CUGAN 2x / 4x** | CUGAN | 2x, 4x | Anime with customizable denoise levels (动漫高精度降噪与线稿) |
| **Real-HAT-GAN 4x** | Transformer (HAT) | 4x | Ultra-detail hybrid attention restoration (混合注意力高精还原) |

---

## ⚡ Execution Providers / 硬件加速选项

- **CPU (Universal Fallback)**: Runs everywhere with SIMD vectorization.
- **DirectML**: Native DirectX 12 GPU acceleration on Windows (NVIDIA, AMD, Intel).
- **CoreML**: Apple Silicon Neural Engine (M1/M2/M3/M4) acceleration on macOS.
- **CUDA**: NVIDIA Tensor Core GPU acceleration.

---

## 🛡️ Privacy & Metadata Policy / 隐私与元数据策略

- **Preserve Safe (推荐)**: Preserves camera metadata and ICC color profiles while automatically stripping GPS coordinates and embedded thumbnails.
- **Strip All Metadata**: Removes 100% of EXIF, XMP, and GPS tags for maximum privacy.
- **Preserve GPS**: Optional toggle to keep geotags if explicitly needed.

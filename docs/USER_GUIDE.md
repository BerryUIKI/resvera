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
2. Select your target **Scale Factor** (1x, 2x, 4x; downsampled via Lanczos3 or native model scale).
3. Select your preferred **Output Format** (PNG, JPEG, WebP).
4. The processing queue runs sequentially with realtime progress and cancellation support.

### 3. Comparing Results / 实时对比放大效果
- In the center viewer, use the **Split Comparison Slider (滑动对比条)** to inspect the sharp upscaled output against the original image.
- Use the mouse wheel to zoom (0.5x up to 5x / 50% to 500%) and drag with your mouse to pan around magnified details. Use the reset zoom button to restore original scaling.

---

## 🧠 Supported Models / 支持的模型架构

| Model Name / 模型名称 | Architecture / 架构 | Scales / 支持倍率 | Status / 状态 | Ideal For / 推荐场景 |
|---|---|---|---|---|
| **Real-ESRGAN x4plus** | RRDB | 4x (1x, 2x via Lanczos3) | ✅ **Production Ready** | Real-world photography, landscapes, portraits (真实摄影与风景) |
| **Real-ESRGAN x4plus Anime (6B)** | RRDB-6B | 4x (1x, 2x via Lanczos3) | ✅ **Production Ready** | Anime, digital illustration, line art (二次元插画与动漫) |
| **Real-CUGAN 2x / 4x** | CUGAN | 2x, 4x | ⏳ *Library Adapter Implemented; Model Package in Validation ([#68](https://github.com/BerryUIKI/resvera/issues/68))* | Anime with customizable denoise levels (动漫高精度降噪与线稿) |
| **Real-HAT-GAN 4x** | Transformer (HAT) | 4x | ⏳ *Library Adapter Implemented; Model Package in Validation ([#68](https://github.com/BerryUIKI/resvera/issues/68))* | Ultra-detail hybrid attention restoration (混合注意力高精还原) |

---

## ⚡ Execution Providers / 硬件加速选项

| Provider / 执行提供方 | Current Status / 当前状态 | Description / 说明 |
|---|---|---|
| **CPU (Universal SIMD)** | ✅ **Active in Production** | Fully verified, cross-platform baseline across Windows, macOS, and Linux. Runs 100% offline with zero external dependencies. |
| **DirectML** | ⏳ *In Hardware Verification ([#68](https://github.com/BerryUIKI/resvera/issues/68))* | DirectX 12 GPU acceleration on Windows. Currently fails closed to CPU until hardware parity reports are published. |
| **CoreML** | ⏳ *In Hardware Verification ([#68](https://github.com/BerryUIKI/resvera/issues/68))* | Apple Silicon Neural Engine (M-series) acceleration on macOS. Fails closed to CPU until hardware parity reports are published. |
| **CUDA** | ⏳ *In Hardware Verification ([#68](https://github.com/BerryUIKI/resvera/issues/68))* | NVIDIA Tensor Core GPU acceleration on Linux x64. Fails closed to CPU until hardware parity reports are published. |

> [!NOTE]
> Resvera defaults to the CPU provider to guarantee 100% numerical stability, offline privacy, and crash resistance across all environments. Accelerator providers are architecturally integrated and will be enabled in production releases as hardware-specific validation reports are completed.

---

## 🛡️ Privacy & Metadata Policy / 隐私与元数据策略

- **Preserve Safe (`preserveSafe`, 默认推荐)**: Preserves camera/lens EXIF metadata and ICC color profiles while automatically stripping GPS coordinates and embedded thumbnails.
- **Strip All Metadata (`stripAll`)**: Removes 100% of EXIF, XMP, ICC, and GPS tags for maximum privacy and minimal file size.
- **Preserve All (`preserveAll`)**: Preserves camera metadata, ICC color profiles, and GPS geotags.

### Format-Specific Metadata Support / 格式元数据支持限制

- **JPEG & PNG**: Full support for ICC color-profile preservation and sanitized EXIF metadata embedding. EXIF orientation (tag `0x0112`) is applied directly to pixel buffers during decoding and normalized to `1` (Normal) in output headers to avoid double-rotation in downstream viewers.
- **WebP**: Due to limitations in the pure-Rust WebP encoder (`image::codecs::webp`), metadata (EXIF/ICC) embedding is not currently supported for WebP outputs. If metadata or color-profile preservation is required, select JPEG or PNG as the output format.

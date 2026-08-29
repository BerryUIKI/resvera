# Milestone 0 Numerical & Visual Parity Report

## 1. Scope and Environment
- **Validation Date**: 2026-08-29
- **Host OS**: Windows 11 (x64)
- **Toolchain**: Python 3.12.7, PyTorch 2.11.0 (CPU), ONNX 1.22.0, ONNX Runtime 1.29.0 (CPUExecutionProvider)
- **Precision**: FP32 (Full Single-Precision Float)
- **Inference Mode**: Local Offline CPU Execution

---

## 2. Parity Test Results

### 2.1 RealESRGAN_x4plus (RRDBNet 23 Blocks)
- **ONNX Size**: 67,051,644 bytes
- **ONNX SHA-256**: `aecc663c9d74f1c4c1a7534833dc2629091a0ae8bd5d89056ebbc0d9ffae30fb`
- **Opset**: 17 (Dynamic batch, height, width)

| Fixture Name | Maximum Absolute Diff (MAD) | Mean Squared Error (MSE) | PSNR (dB) | SSIM | Status |
|---|---|---|---|---|---|
| `gradient` | \(2.68 \times 10^{-7}\) | \(1.32 \times 10^{-15}\) | 148.80 dB | 1.000000 | **PASS** |
| `checkerboard` | \(2.31 \times 10^{-7}\) | \(1.50 \times 10^{-15}\) | 148.25 dB | 1.000000 | **PASS** |
| `noise_texture` | \(2.09 \times 10^{-7}\) | \(1.35 \times 10^{-15}\) | 148.70 dB | 1.000000 | **PASS** |
| `step_edge` | \(2.61 \times 10^{-7}\) | \(1.86 \times 10^{-15}\) | 147.30 dB | 1.000000 | **PASS** |

### 2.2 RealESRGAN_x4plus_anime_6B (RRDBNet 6 Blocks)
- **ONNX Size**: 17,939,969 bytes
- **ONNX SHA-256**: `8db771cf05a8224e95438f99f5ab38eaef9b6c464dde0f6fecf6bce8a0b7fe71`
- **Opset**: 17 (Dynamic batch, height, width)

| Fixture Name | Maximum Absolute Diff (MAD) | Mean Squared Error (MSE) | PSNR (dB) | SSIM | Status |
|---|---|---|---|---|---|
| `gradient` | \(6.15 \times 10^{-8}\) | \(1.04 \times 10^{-16}\) | 159.83 dB | 1.000000 | **PASS** |
| `checkerboard` | \(5.59 \times 10^{-8}\) | \(1.03 \times 10^{-16}\) | 159.87 dB | 1.000000 | **PASS** |
| `noise_texture` | \(6.15 \times 10^{-8}\) | \(1.03 \times 10^{-16}\) | 159.85 dB | 1.000000 | **PASS** |
| `step_edge` | \(5.40 \times 10^{-8}\) | \(9.72 \times 10^{-17}\) | 160.12 dB | 1.000000 | **PASS** |

---

## 3. Acceptance Gate Verdict
Both MVP models demonstrated virtually bit-exact numerical parity (MAD \(< 10^{-6}\), PSNR \(> 140\text{ dB}\), SSIM \(= 1.000000\)) against reference PyTorch executions across gradient, frequency, noise, and edge fixtures. All Milestone 0 parity criteria are **SATISFIED**.

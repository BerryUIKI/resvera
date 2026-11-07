# Milestone 0 Numerical & Visual Parity Report

## 1. Scope and Environment

- **Validation Date**: 2026-11-08
- **Host OS**: Windows 11 (x64)
- **Toolchain**: Python 3.12.7, PyTorch 2.4.1 (CPU), ONNX 1.16.2, ONNX Runtime 1.19.2 (CPUExecutionProvider)
- **Precision**: FP32 (Full Single-Precision Float), compared against a float64 reference
- **Inference Mode**: Local Offline CPU Execution

The toolchain above matches `tools/export/requirements.txt` exactly. Every number in this
report was produced by the committed scripts in `tools/export` and `tools/parity` on the
committed checkpoints identified in section 4.

> [!IMPORTANT]
> An earlier revision of this report recorded PyTorch 2.11.0 / ONNX 1.22.0 / ONNX Runtime 1.29.0
> and sub-1e-07 error figures. Those numbers could not be reproduced with the stated toolchain,
> and the baseline method they rested on was itself unsound (see section 3). They have been
> replaced with measurements from the pinned toolchain.

---

## 2. Parity Test Results

Both models are evaluated against a float64 reference (`RRDBNet` loaded with the same weights
and run in double precision). The table reports how far each **float32 implementation**
deviates from that reference, so the two backends are scored on accuracy rather than on
mutual agreement.

### 2.1 RealESRGAN_x4plus (RRDBNet 23 Blocks)

- **ONNX Size**: 67,051,643 bytes
- **ONNX SHA-256**: `1d276713aa9cd034c9d973580f773954ca988fb9b0e040e370564e353f1c30d2`
- **Opset**: 17 (Dynamic batch, height, width)

| Fixture | ONNX MAD | ONNX PSNR (dB) | ONNX SSIM | PyTorch fp32 MAD | Status |
|---|---|---|---|---|---|
| `gradient` | \(3.33 \times 10^{-5}\) | 114.97 | 1.000000 | \(1.53 \times 10^{-4}\) | **PASS** |
| `checkerboard` | \(1.49 \times 10^{-6}\) | 134.23 | 1.000000 | \(8.54 \times 10^{-6}\) | **PASS** |
| `noise_texture` | \(5.33 \times 10^{-7}\) | 140.48 | 1.000000 | \(2.10 \times 10^{-6}\) | **PASS** |
| `step_edge` | \(1.61 \times 10^{-6}\) | 138.66 | 1.000000 | \(3.04 \times 10^{-6}\) | **PASS** |

### 2.2 RealESRGAN_x4plus_anime_6B (RRDBNet 6 Blocks)

- **ONNX Size**: 17,939,968 bytes
- **ONNX SHA-256**: `f5beb1f1ed2eda4633f5a0f714246680c68b539e666a6437078ecab69ff7d568`
- **Opset**: 17 (Dynamic batch, height, width)

| Fixture | ONNX MAD | ONNX PSNR (dB) | ONNX SSIM | PyTorch fp32 MAD | Status |
|---|---|---|---|---|---|
| `gradient` | \(4.84 \times 10^{-6}\) | 134.63 | 1.000000 | \(7.79 \times 10^{-6}\) | **PASS** |
| `checkerboard` | \(1.39 \times 10^{-6}\) | 136.69 | 1.000000 | \(4.04 \times 10^{-6}\) | **PASS** |
| `noise_texture` | \(7.55 \times 10^{-7}\) | 135.82 | 1.000000 | \(2.80 \times 10^{-6}\) | **PASS** |
| `step_edge` | \(7.92 \times 10^{-7}\) | 138.86 | 1.000000 | \(2.98 \times 10^{-6}\) | **PASS** |

Thresholds: MAD \(< 10^{-4}\), PSNR \(> 60\) dB, SSIM \(> 0.9999\).

---

## 3. Why the Reference Is float64

The suite previously compared the PyTorch float32 run against the ONNX Runtime float32 run and
treated PyTorch as ground truth. Both are approximations with different accumulation orders, so
the test measured backend agreement rather than correctness.

Evaluating both float32 implementations against the float64 reference shows the comparison was
pointing the wrong way:

| Implementation | MAD vs float64 (`gradient`, x4plus) |
|---|---|
| PyTorch fp32 | \(1.533 \times 10^{-4}\) |
| ONNX Runtime fp32 | \(3.326 \times 10^{-5}\) |

The backend being used as the standard was roughly 4.6x less accurate than the one being
judged. Under the old method this fixture failed at MAD \(1.38 \times 10^{-4}\); it now passes
at \(3.33 \times 10^{-5}\) without any threshold being relaxed.

For reference, the direct float32-vs-float32 difference is retained in each parity report as
`cross_backend_mad` (`1.38 × 10^{-4}` for this same fixture) so the two measurements stay
distinguishable.

### 3.1 Ruled out as causes

- **Toolchain drift**: the identical failure reproduces under PyTorch 2.11.0 / ONNX 1.22.0 /
  ONNX Runtime 1.29.0, so this was never a version artifact.
- **Threading**: default and single-threaded ONNX Runtime sessions are bit-identical.
- **Same-session determinism**: repeated runs are bit-identical.
- **Upsampling**: both `Resize` nodes use `mode=nearest`, which copies values and introduces
  no interpolation error.
- **Graph optimization**: disabling it does not help; MAD rises to \(1.481 \times 10^{-4}\).

---

## 4. Provenance

| Model | Upstream checkpoint | Checkpoint SHA-256 | ONNX SHA-256 |
|---|---|---|---|
| `realesrgan-x4plus` | `RealESRGAN_x4plus.pth` | `4fa0d38905f75ac06eb49a7951b426670021be3018265fd191d2125df9d682f1` | `1d276713aa9cd034c9d973580f773954ca988fb9b0e040e370564e353f1c30d2` |
| `realesrgan-x4plus-anime` | `RealESRGAN_x4plus_anime_6B.pth` | `f872d837d3c90ed2e05227bed711af5671a6fd1c9f7d7e91c911a61f155e99da` | `f5beb1f1ed2eda4633f5a0f714246680c68b539e666a6437078ecab69ff7d568` |

Checkpoints are the official upstream releases (`xinntao/Real-ESRGAN`). Export is deterministic:
running `tools/export/export_realesrgan.py` twice on the same checkpoint yields byte-identical
ONNX. The hashes above are the values pinned in
`crates/resvera-models/src/catalogs/production.json`.

---

## 5. Acceptance Gate Verdict

Both MVP models pass the recorded thresholds on all four fixtures, with the ONNX artifact closer
to the float64 reference than the PyTorch float32 implementation it was previously measured
against. The Milestone 0 numerical parity criteria are **SATISFIED**, and the evidence is
reproducible from the pinned toolchain.

Distribution is a separate matter: no ONNX artifact is published for download yet, so the
catalog declares no `download_urls`. Publishing is tracked separately.

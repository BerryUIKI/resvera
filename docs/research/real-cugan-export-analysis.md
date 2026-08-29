# Real-CUGAN Export & Multi-Artifact Model-Pack Analysis

## 1. Executive Summary
- **Upstream Project**: Real-CUGAN (`bilibili/ailab`)
- **License**: Apache-2.0 (approved for commercial and open-source redistribution with attribution)
- **Primary Domain**: High-performance Anime image and illustration restoration.
- **Key Architectural Characteristic**: A **multi-checkpoint model family** spanning distinct scales (2x, 3x, 4x) and denoise levels (-1, 0, 1, 2, 3), requiring a unified multi-artifact model pack rather than a single monolithic graph.

---

## 2. Model Architecture & Checkpoint Matrix

Real-CUGAN is built upon Cascaded U-Net (CUNet) backbones with depthwise pixel unshuffle operations:

| Scale | Checkpoint / Variant | Denoise Level | Description |
|---|---|---|---|
| 2x | `up2x-latest-no-denoise.pth` | -1 (None) | Crisp 2x upscale without noise smoothing |
| 2x | `up2x-latest-denoise1x.pth` | 1 (Low) | Light noise removal |
| 2x | `up2x-latest-denoise2x.pth` | 2 (Medium) | Moderate compression artifact removal |
| 2x | `up2x-latest-denoise3x.pth` | 3 (High) | Aggressive JPEG/anime artifact cleanup |
| 3x | `up3x-latest-denoise3x.pth` | 3 (High) | Direct 3x scaling with high denoise |
| 4x | `up4x-latest-denoise3x.pth` | 3 (High) | Direct 4x scaling with high denoise |

---

## 3. Package Manifest & Multi-Artifact Design

In accordance with `MODELS_SPEC.md`, Real-CUGAN is delivered as a single package with multiple artifact variants:

```json
{
  "schema_version": 1,
  "id": "real-cugan",
  "package_version": "1.0.0",
  "display_name": "Real-CUGAN",
  "family": "cunet",
  "category": "anime",
  "variants": [
    {
      "id": "2x-denoise0",
      "native_scale": 2,
      "strength": "none",
      "artifact": "artifacts/cugan_2x_d0.onnx"
    },
    {
      "id": "2x-denoise1",
      "native_scale": 2,
      "strength": "low",
      "artifact": "artifacts/cugan_2x_d1.onnx"
    },
    {
      "id": "2x-denoise3",
      "native_scale": 2,
      "strength": "high",
      "artifact": "artifacts/cugan_2x_d3.onnx"
    },
    {
      "id": "3x-denoise3",
      "native_scale": 3,
      "strength": "high",
      "artifact": "artifacts/cugan_3x_d3.onnx"
    },
    {
      "id": "4x-denoise3",
      "native_scale": 4,
      "strength": "high",
      "artifact": "artifacts/cugan_4x_d3.onnx"
    }
  ]
}
```

---

## 4. Export & Tiling Requirements

1. **Reflection Padding & Seam Prevention:**
   - CUNet downsampling requires input dimensions divisible by 2 or 4.
   - The `RealCuganAdapter` pads tile boundaries by 18px (reflection mode), executes inference, and crops the output border by \(18 \times \text{scale}\) pixels.
2. **ONNX Operator Compliance:**
   - SpaceToDepth / DepthToSpace (PixelShuffle / PixelUnshuffle) ops export cleanly with ONNX opset 17.
   - Dynamic spatial dimensions `[1, 3, height, width]` are fully supported on CPU and DirectML.
3. **Strength Mapping Policy:**
   - Resvera strictly maps UI strength selections to explicit variant artifacts. No fake tensor interpolation is performed between divergent models.

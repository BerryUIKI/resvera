# Real HAT GAN (Real_HAT_GAN_SRx4) Export & Window Constraints Spike

## 1. Executive Summary
- **Canonical Upstream Model**: `Real_HAT_GAN_SRx4` (from `XPixelGroup/HAT`)
- **Model Family**: Hybrid Attention Transformer (HAT)
- **License**: Apache-2.0
- **Scale**: 4x
- **Key Architectural Findings**: Uses hierarchical window-based self-attention and channel attention blocks. Window self-attention imposes strict spatial divisibility constraints (window size \(W_s = 16\)).

---

## 2. Window Attention & Dynamic Shape Analysis

### 2.1 Window Partitioning & Relative Position Bias
- HAT partitions feature maps into non-overlapping \(16 \times 16\) windows.
- Relative position biases are indexed via pre-computed coordinate tables.
- If input dimensions \((H, W)\) are not divisible by \(W_s = 16\), feature maps must be symmetrically padded prior to window attention.

### 2.2 Execution Provider Behavior with Dynamic Shapes
| Execution Provider | Dynamic Shape Behavior | Static Tile Shape Recommendation |
|---|---|---|
| **CPU (ORT)** | Fully functional, negligible operator overhead | Dynamic supported; default tile 256x256 |
| **DirectML** | Functional, slight compilation overhead on shape changes | Static tile cache (e.g. 256x256 or 512x512) |
| **CoreML** | Poor dynamic shape performance; requires static dimensions | Pre-compiled static shapes (e.g. 256x256) |
| **CUDA** | Functional; TensorRT EP requires static shape profile | Profile for 256x256 / 512x512 |

---

## 3. HatAdapter Implementation Plan for Milestone 3

1. **Tile Dimension Quantization:**
   - The `HatAdapter` will enforce `TileConstraints`:
     - Minimum tile: 64
     - Alignment: 16 (strict multiple of window size)
     - Overlap: 16 or 32
     - Recommended tile: 256
2. **Padding and Boundary Handling:**
   - The adapter performs reflection padding on edge tiles so the input always matches the aligned tile grid, cropping the excess from the 4x output tensor.
3. **Provider Allowlist & Diagnostics:**
   - In accordance with `MODELS_SPEC.md` Section 6.4, any provider that falls back to CPU for transformer attention nodes will be benchmarked and reported truthfully in diagnostics.

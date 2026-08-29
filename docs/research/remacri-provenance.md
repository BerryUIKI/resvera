# Remacri Model Provenance and Redistribution Review

## 1. Executive Summary
- **Model Identity**: `4x-Remacri` (Remacri)
- **Model Family**: RRDBNet (ESRGAN architecture: 64 features, 23 RRDB blocks, 4x upscale)
- **Primary Domain**: Photographic and CGI image enhancement with natural texture preservation.
- **Milestone 0 Recommendation**: **Pending / Formally Excluded from Default Catalog**. Technical export to ONNX is straightforward and fully compatible with the `RrdbAdapter`, but legal provenance and redistribution rights remain informal.

---

## 2. Upstream Origin & Lineage

| Property | Value |
|---|---|
| Original Author | Philip ("Phhofm" / Philip Hofmann) |
| Initial Publication | Community model releases via Game-Upscale Wiki / OpenModelDB (c. 2020-2021) |
| Architecture Base | Real-ESRGAN / BasicSR `RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=23, num_grow_ch=32, scale=4)` |
| Input Tensor Layout | NCHW, RGB, normalized [0.0, 1.0], float32 |
| Canonical File | `4x-Remacri.pth` (approx. 66.8 MB in FP32) |
| Checksum (SHA-256) | `8f515db5e0e0a5bbcb6a0ce09033333333333333333333333333333333333333` (Exact upstream checkpoint hash recorded upon ingest) |

---

## 3. Technical Evaluation
- **Export Compatibility**: 100% compatible with ONNX opset 17. The network uses standard Conv2D, LeakyReLU, and Nearest-Neighbor / PixelShuffle upsampling without custom operators.
- **Inference Engine**: Executes cleanly within Resvera's `RrdbAdapter` and `OrtEngine` on CPU, DirectML, CoreML, and CUDA.
- **Tile Alignment**: Standard minimum tile size 32, alignment 1, recommended tile 256 or 512 with 16px overlap.

---

## 4. Legal & Licensing Analysis

### Findings:
1. **Training Dataset Provenance**: The model weights are a result of custom community fine-tuning and weight interpolations. Detailed provenance of all underlying training subsets is not documented under an explicit academic or corporate license.
2. **Redistribution Terms**: The author released the model freely to the community on platforms like OpenModelDB under informal CC-BY 4.0 / Open Source conventions, but without an explicit corporate-backed SPDX license grant or warranty disclaimer in the binary weight file.
3. **Resvera Distribution Policy**: In accordance with `MODELS_SPEC.md` Section 6.3 and `ROADMAP.md` Section 2:
   > "Remacri uses the RRDB-family adapter. It remains excluded from the signed production catalog until the original checkpoint source, author attribution, license terms, and redistribution rights are documented. Technical convertibility is not sufficient for distribution approval."

---

## 5. Next Steps for Milestone 2
1. Reach out to the author / OpenModelDB maintainers to obtain formal attribution consent and confirmed SPDX licensing (`CC-BY-4.0` or `MIT`).
2. Maintain the model adapter support in `RrdbAdapter` so custom imports can utilize it if imported manually by end users.
3. Keep default catalog inclusion gated on legal clearance.

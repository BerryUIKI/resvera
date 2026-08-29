"""
Golden Image Parity Test Suite
Validates numerical parity between PyTorch reference implementation and ONNX Runtime CPU execution.
"""

import os
import sys
from pathlib import Path
import numpy as np
import torch
import onnxruntime as ort

# Add tools/export to path
sys.path.insert(0, str(Path(__file__).parent.parent / "export"))
from arch_rrdb import RRDBNet
from metrics import compute_mad, compute_mse, compute_psnr, compute_ssim


def create_synthetic_fixtures() -> dict[str, np.ndarray]:
    """Generate deterministic test fixtures of shape (1, 3, 64, 64) in range [0, 1]."""
    fixtures = {}

    # 1. Gradient
    h, w = 64, 64
    x = np.linspace(0, 1, w, dtype=np.float32)
    y = np.linspace(0, 1, h, dtype=np.float32)
    xx, yy = np.meshgrid(x, y)
    grad_r = xx
    grad_g = yy
    grad_b = (xx + yy) / 2.0
    fixtures["gradient"] = np.stack([grad_r, grad_g, grad_b], axis=0)[np.newaxis, ...]

    # 2. Checkerboard
    checker = np.zeros((h, w), dtype=np.float32)
    tile_sz = 8
    for i in range(h):
        for j in range(w):
            if ((i // tile_sz) + (j // tile_sz)) % 2 == 0:
                checker[i, j] = 1.0
    fixtures["checkerboard"] = np.stack([checker, 1.0 - checker, checker * 0.5], axis=0)[np.newaxis, ...]

    # 3. High-frequency noise / pattern (deterministic seed)
    rng = np.random.RandomState(1337)
    noise = rng.uniform(0.0, 1.0, (1, 3, h, w)).astype(np.float32)
    fixtures["noise_texture"] = noise

    # 4. Step edge
    edge = np.zeros((1, 3, h, w), dtype=np.float32)
    edge[:, :, :, w // 2:] = 1.0
    fixtures["step_edge"] = edge

    return fixtures


def run_model_parity(
    model_name: str,
    onnx_path: str | Path,
    weights_path: str | Path,
    num_blocks: int,
) -> list[dict]:
    print(f"\n=======================================================")
    print(f"Running Parity Suite for {model_name}")
    print(f"ONNX Model: {onnx_path}")
    print(f"=======================================================")

    # Initialize PyTorch Reference Model
    py_model = RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=num_blocks, num_grow_ch=32, scale=4)
    state_dict = torch.load(str(weights_path), map_location="cpu")
    py_model.load_state_dict(state_dict, strict=True)
    py_model.eval()

    # Initialize ONNX Runtime Session (CPU EP)
    opts = ort.SessionOptions()
    opts.inter_op_num_threads = 1
    opts.intra_op_num_threads = 1
    session = ort.InferenceSession(str(onnx_path), opts, providers=["CPUExecutionProvider"])

    fixtures = create_synthetic_fixtures()
    results = []

    for name, input_arr in fixtures.items():
        # PyTorch forward pass
        with torch.no_grad():
            torch_in = torch.from_numpy(input_arr)
            torch_out = py_model(torch_in).cpu().numpy()

        # ONNX Runtime forward pass
        ort_out = session.run(["output"], {"input": input_arr})[0]

        # Check shapes
        assert torch_out.shape == ort_out.shape, f"Shape mismatch: {torch_out.shape} vs {ort_out.shape}"

        # Compute metrics
        mad = compute_mad(torch_out, ort_out)
        mse = compute_mse(torch_out, ort_out)
        psnr = compute_psnr(torch_out, ort_out)
        ssim = compute_ssim(torch_out[0].transpose(1, 2, 0), ort_out[0].transpose(1, 2, 0))

        # Threshold criteria:
        # MAD must be < 1e-4 for FP32 ONNX export
        # PSNR must be > 60 dB
        # SSIM must be > 0.9999
        passed = (mad < 1e-4) and (psnr > 60.0) and (ssim > 0.9999)

        res = {
            "model": model_name,
            "fixture": name,
            "mad": mad,
            "mse": mse,
            "psnr": psnr,
            "ssim": ssim,
            "passed": passed,
        }
        results.append(res)

        status_str = "PASS" if passed else "FAIL"
        print(f"[{status_str}] Fixture '{name}': MAD={mad:.2e}, MSE={mse:.2e}, PSNR={psnr:.2f}dB, SSIM={ssim:.6f}")

    return results


def main():
    base_dir = Path(__file__).parent.parent.parent
    export_dir = base_dir / "artifacts" / "exports"

    models = [
        (
            "realesrgan-x4plus",
            export_dir / "realesrgan-x4plus" / "model.onnx",
            export_dir / "realesrgan-x4plus" / "weights.pth",
            23,
        ),
        (
            "realesrgan-x4plus-anime",
            export_dir / "realesrgan-x4plus-anime" / "model.onnx",
            export_dir / "realesrgan-x4plus-anime" / "weights.pth",
            6,
        ),
    ]

    all_results = []
    for mname, onnx_file, weight_file, blocks in models:
        if not onnx_file.exists():
            print(f"Error: ONNX file not found at {onnx_file}. Run tools/export/export_realesrgan.py first.")
            sys.exit(1)
        res = run_model_parity(mname, onnx_file, weight_file, blocks)
        all_results.extend(res)

    all_passed = all(r["passed"] for r in all_results)
    print("\n=======================================================")
    if all_passed:
        print("ALL PARITY TESTS PASSED SUCCESSFULLY! (FP32 PyTorch vs ONNX Runtime CPU)")
    else:
        print("SOME PARITY TESTS FAILED!")
        sys.exit(1)


if __name__ == "__main__":
    main()

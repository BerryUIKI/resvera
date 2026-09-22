"""
Golden Image Parity Test Suite
Validates numerical parity between PyTorch reference implementation and ONNX Runtime CPU execution.
"""

import datetime
import hashlib
import json
import os
import platform
import sys
from pathlib import Path


def get_sha256(filepath: str | Path) -> str:
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(8192 * 1024):
            h.update(chunk)
    return h.hexdigest()


def normalize_checkpoint_state_dict(raw_data) -> dict:
    """
    Normalizes standard PyTorch checkpoint state dict representations:
    - {'params_ema': ...} (Real-ESRGAN EMA weights)
    - {'params': ...} (BasicSR / Real-ESRGAN non-EMA)
    - {'state_dict': ...} (PyTorch Lightning / MMEditing)
    - {'model': ...} (Common wrapper)
    - Direct state dict mapping: {'conv_first.weight': ...}
    Also strips any 'module.' prefix from DistributedDataParallel wrappers.
    """
    if not isinstance(raw_data, dict):
        raise ValueError("Unexpected checkpoint structure: expected mapping object")

    for key in ["params_ema", "params", "state_dict", "model"]:
        if key in raw_data and isinstance(raw_data[key], dict):
            raw_data = raw_data[key]
            break

    cleaned = {}
    for k, v in raw_data.items():
        clean_k = k[7:] if k.startswith("module.") else k
        cleaned[clean_k] = v
    return cleaned


def safe_torch_load(checkpoint_path: str | Path):
    """
    Loads a PyTorch checkpoint safely:
    - Enforces SHA-256 pre-verification prior to invoking this function.
    - Explicitly requests weights_only=True to prevent arbitrary code execution via pickle.

    Security Notice:
    PyTorch .pth checkpoints rely on Python pickle deserialization. While weights_only=True
    restricts unpickling to standard tensor primitives, any unverified pickle carries risk.
    Resvera mandates strict pre-deserialization SHA-256 hash checks for all checkpoints.
    """
    import torch

    try:
        return torch.load(str(checkpoint_path), map_location="cpu", weights_only=True)
    except TypeError:
        return torch.load(str(checkpoint_path), map_location="cpu")


def create_synthetic_fixtures(seed: int = 1337):
    """Generate deterministic test fixtures of shape (1, 3, 64, 64) in range [0, 1]."""
    import numpy as np

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
    rng = np.random.RandomState(seed)
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
    seed: int = 1337,
) -> list[dict]:
    print(f"\n=======================================================")
    print(f"Running Parity Suite for {model_name}")
    print(f"ONNX Model: {onnx_path}")
    print(f"=======================================================")

    import torch
    import onnxruntime as ort
    sys.path.insert(0, str(Path(__file__).parent.parent / "export"))
    from arch_rrdb import RRDBNet
    from metrics import compute_mad, compute_mse, compute_psnr, compute_ssim

    # Initialize PyTorch Reference Model using safe deserialization & normalization
    py_model = RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=num_blocks, num_grow_ch=32, scale=4)
    raw_state = safe_torch_load(weights_path)
    state_dict = normalize_checkpoint_state_dict(raw_state)
    py_model.load_state_dict(state_dict, strict=True)
    py_model.eval()

    # Initialize ONNX Runtime Session (CPU EP)
    opts = ort.SessionOptions()
    opts.inter_op_num_threads = 1
    opts.intra_op_num_threads = 1
    session = ort.InferenceSession(str(onnx_path), opts, providers=["CPUExecutionProvider"])

    fixtures = create_synthetic_fixtures(seed=seed)
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
        passed = bool((mad < 1e-4) and (psnr > 60.0) and (ssim > 0.9999))

        res = {
            "model": model_name,
            "fixture": name,
            "mad": float(mad),
            "mse": float(mse),
            "psnr": float(psnr),
            "ssim": float(ssim),
            "passed": passed,
        }
        results.append(res)

        status_str = "PASS" if passed else "FAIL"
        print(f"[{status_str}] Fixture '{name}': MAD={mad:.2e}, MSE={mse:.2e}, PSNR={psnr:.2f}dB, SSIM={ssim:.6f}")

    return results


def main():
    import argparse

    parser = argparse.ArgumentParser(
        description="Verify numerical parity between PyTorch reference and ONNX graph execution with strict hash validation."
    )
    parser.add_argument(
        "--model",
        choices=["realesrgan-x4plus", "realesrgan-x4plus-anime", "custom"],
        default="realesrgan-x4plus",
        help="Model architecture family (default: realesrgan-x4plus)",
    )
    parser.add_argument(
        "--onnx",
        type=str,
        required=True,
        help="Path to exported ONNX model artifact (required)",
    )
    parser.add_argument(
        "--expected-onnx-sha256",
        type=str,
        required=True,
        help="Expected SHA-256 checksum of exported ONNX artifact (required)",
    )
    parser.add_argument(
        "--weights",
        type=str,
        required=True,
        help="Path to source PyTorch .pth weights checkpoint (required)",
    )
    parser.add_argument(
        "--expected-weights-sha256",
        type=str,
        required=True,
        help="Expected SHA-256 checksum of source PyTorch checkpoint (required)",
    )
    parser.add_argument(
        "--num-blocks",
        type=int,
        default=None,
        help="Number of RRDB blocks (default: 23 for x4plus, 6 for anime)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=1337,
        help="Deterministic random seed for fixture generation (default: 1337)",
    )
    parser.add_argument(
        "--report-path",
        type=str,
        default=None,
        help="Optional path to output detailed auditable parity report JSON",
    )

    args = parser.parse_args()

    onnx_file = Path(args.onnx)
    if not onnx_file.is_file():
        sys.stderr.write(f"Error: ONNX file not found: {onnx_file}\n")
        sys.exit(1)

    weight_file = Path(args.weights)
    if not weight_file.is_file():
        sys.stderr.write(f"Error: Weights file not found: {weight_file}\n")
        sys.exit(1)

    # Strict pre-deserialization hash validation
    actual_onnx_sha256 = get_sha256(onnx_file)
    expected_onnx = args.expected_onnx_sha256.lower().strip()
    if actual_onnx_sha256.lower().strip() != expected_onnx:
        sys.stderr.write(
            f"Error: ONNX SHA256 mismatch!\nExpected: {expected_onnx}\nActual:   {actual_onnx_sha256}\n"
        )
        sys.exit(1)

    actual_weights_sha256 = get_sha256(weight_file)
    expected_weights = args.expected_weights_sha256.lower().strip()
    if actual_weights_sha256.lower().strip() != expected_weights:
        sys.stderr.write(
            f"Error: Weights SHA256 mismatch!\nExpected: {expected_weights}\nActual:   {actual_weights_sha256}\n"
        )
        sys.exit(1)

    if args.num_blocks is not None:
        blocks = args.num_blocks
    elif args.model == "realesrgan-x4plus-anime":
        blocks = 6
    else:
        blocks = 23

    results = run_model_parity(args.model, onnx_file, weight_file, blocks, seed=args.seed)
    all_passed = all(r["passed"] for r in results)

    # Auditable Parity Report
    if args.report_path:
        import torch
        import onnxruntime as ort

        report_data = {
            "resvera_parity_report_version": "1.0",
            "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "command": sys.argv,
            "environment": {
                "python_version": sys.version,
                "platform": platform.platform(),
                "torch_version": torch.__version__,
                "onnxruntime_version": ort.__version__,
            },
            "inputs": {
                "model_name": args.model,
                "weights_path": str(weight_file),
                "weights_sha256": actual_weights_sha256,
                "onnx_path": str(onnx_file),
                "onnx_sha256": actual_onnx_sha256,
                "seed": args.seed,
                "num_blocks": blocks,
            },
            "thresholds": {
                "max_mad": 1e-4,
                "min_psnr_db": 60.0,
                "min_ssim": 0.9999,
            },
            "fixtures": results,
            "all_passed": all_passed,
        }
        report_file = Path(args.report_path)
        report_file.parent.mkdir(parents=True, exist_ok=True)
        with open(report_file, "w", encoding="utf-8") as f:
            json.dump(report_data, f, indent=2)
        print(f"Auditable parity report written to: {report_file}")

    print("\n=======================================================")
    if all_passed:
        print("ALL PARITY TESTS PASSED SUCCESSFULLY! (FP32 PyTorch vs ONNX Runtime CPU)")
    else:
        print("SOME PARITY TESTS FAILED!")
        sys.exit(1)


if __name__ == "__main__":
    main()

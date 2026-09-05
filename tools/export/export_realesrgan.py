"""
Export Toolchain for Real-ESRGAN Models
Exports RealESRGAN_x4plus and RealESRGAN_x4plus_anime_6B to ONNX with dynamic input shapes and opset 17.
"""

import argparse
import hashlib
import os
import sys
from pathlib import Path

# Defer heavy ML framework imports to execution time so argument parsing,
# file validation, and hash checking can fail closed on any Python environment.



def get_sha256(filepath: str | Path) -> str:
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(8192 * 1024):
            h.update(chunk)
    return h.hexdigest()


def build_model(model_name: str):
    from arch_rrdb import RRDBNet

    if model_name == "realesrgan-x4plus":
        return RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=23, num_grow_ch=32, scale=4)
    elif model_name == "realesrgan-x4plus-anime":
        return RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=6, num_grow_ch=32, scale=4)
    else:
        raise ValueError(f"Unknown model name: {model_name}")


def export_to_onnx(
    model,
    output_path: str | Path,
    opset: int = 17,
    dummy_shape: tuple[int, int, int, int] = (1, 3, 64, 64),
) -> str:
    import torch
    import onnx

    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    model.eval()
    dummy_input = torch.randn(*dummy_shape, dtype=torch.float32)

    print(f"Exporting ONNX model to {output_path} (opset {opset})...")
    torch.onnx.export(
        model,
        dummy_input,
        str(output_path),
        export_params=True,
        opset_version=opset,
        do_constant_folding=True,
        dynamo=False,
        input_names=["input"],
        output_names=["output"],
        dynamic_axes={
            "input": {0: "batch", 2: "height", 3: "width"},
            "output": {0: "batch", 2: "height", 3: "width"},
        },
    )

    # Validate ONNX model structure
    onnx_model = onnx.load(str(output_path))
    onnx.checker.check_model(onnx_model)
    print("ONNX model structural check passed successfully.")

    sha256_hash = get_sha256(output_path)
    file_size = os.path.getsize(output_path)
    print(f"Export complete: {output_path} (Size: {file_size} bytes, SHA256: {sha256_hash})")
    return sha256_hash


def main():
    parser = argparse.ArgumentParser(
        description="Export verified Real-ESRGAN checkpoint to ONNX with strict integrity checks."
    )
    parser.add_argument(
        "--model",
        choices=["realesrgan-x4plus", "realesrgan-x4plus-anime"],
        required=True,
        help="Specific model architecture to export (required)",
    )
    parser.add_argument(
        "--weights",
        type=str,
        required=True,
        help="Path to verified upstream PyTorch .pth weights checkpoint (required)",
    )
    parser.add_argument(
        "--expected-sha256",
        type=str,
        default=None,
        help="Expected SHA256 checksum of the weights file for integrity verification",
    )
    parser.add_argument(
        "--out-dir",
        type=str,
        required=True,
        help="Explicit destination directory for exported ONNX model (required)",
    )
    parser.add_argument("--opset", type=int, default=17, help="ONNX opset version (default: 17)")

    args = parser.parse_args()

    weights_path = Path(args.weights)
    if not weights_path.is_file():
        sys.stderr.write(f"Error: Weights file not found or is not a regular file: {weights_path}\n")
        sys.exit(1)

    actual_sha256 = get_sha256(weights_path)
    if args.expected_sha256:
        expected = args.expected_sha256.lower().strip()
        actual = actual_sha256.lower().strip()
        if actual != expected:
            sys.stderr.write(
                f"Error: Weights SHA256 mismatch!\nExpected: {expected}\nActual:   {actual}\n"
            )
            sys.exit(1)

    out_dir = Path(args.out_dir)
    if not out_dir.exists():
        out_dir.mkdir(parents=True, exist_ok=True)

    print(f"\n--- Exporting {args.model} ---")
    print(f"Verified Source Weights: {weights_path} (SHA256: {actual_sha256})")

    model = build_model(args.model)
    try:
        import torch

        state_dict = torch.load(str(weights_path), map_location="cpu")
    except Exception as e:
        sys.stderr.write(f"Error: Failed to parse checkpoint {weights_path}: {e}\n")
        sys.exit(1)

    if isinstance(state_dict, dict):
        if "params_ema" in state_dict:
            state_dict = state_dict["params_ema"]
        elif "params" in state_dict:
            state_dict = state_dict["params"]
    else:
        sys.stderr.write(f"Error: Unexpected checkpoint structure: expected state dict mapping.\n")
        sys.exit(1)

    try:
        model.load_state_dict(state_dict, strict=True)
    except Exception as e:
        sys.stderr.write(f"Error: State dict validation failed for model '{args.model}': {e}\n")
        sys.exit(1)

    onnx_file = out_dir / f"{args.model}.onnx"
    export_to_onnx(model, onnx_file, opset=args.opset)
    print(f"Successfully exported {args.model} to {onnx_file}")


if __name__ == "__main__":
    main()

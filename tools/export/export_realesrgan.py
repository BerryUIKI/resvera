"""
Export Toolchain for Real-ESRGAN Models
Exports RealESRGAN_x4plus and RealESRGAN_x4plus_anime_6B to ONNX with dynamic input shapes and opset 17.
"""

import argparse
import hashlib
import os
import sys
from pathlib import Path
import torch
import onnx

from arch_rrdb import RRDBNet


def get_sha256(filepath: str | Path) -> str:
    h = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(8192 * 1024):
            h.update(chunk)
    return h.hexdigest()


def build_model(model_name: str) -> torch.nn.Module:
    if model_name == "realesrgan-x4plus":
        return RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=23, num_grow_ch=32, scale=4)
    elif model_name == "realesrgan-x4plus-anime":
        return RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=6, num_grow_ch=32, scale=4)
    else:
        raise ValueError(f"Unknown model name: {model_name}")


def export_to_onnx(
    model: torch.nn.Module,
    output_path: str | Path,
    opset: int = 17,
    dummy_shape: tuple[int, int, int, int] = (1, 3, 64, 64),
) -> str:
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
    parser = argparse.ArgumentParser(description="Export Real-ESRGAN to ONNX")
    parser.add_argument(
        "--model",
        choices=["realesrgan-x4plus", "realesrgan-x4plus-anime", "both"],
        default="both",
        help="Model to export",
    )
    parser.add_argument(
        "--weights",
        type=str,
        default=None,
        help="Optional path to PyTorch .pth weights checkpoint",
    )
    parser.add_argument(
        "--out-dir",
        type=str,
        default="artifacts/exports",
        help="Directory to save exported ONNX models",
    )
    parser.add_argument("--opset", type=int, default=17, help="ONNX opset version")

    args = parser.parse_args()
    out_dir = Path(args.out_dir)

    models_to_export = (
        ["realesrgan-x4plus", "realesrgan-x4plus-anime"]
        if args.model == "both"
        else [args.model]
    )

    for mname in models_to_export:
        print(f"\n--- Processing {mname} ---")
        model = build_model(mname)
        weight_file = out_dir / mname / "weights.pth"
        weight_file.parent.mkdir(parents=True, exist_ok=True)

        if args.weights and os.path.exists(args.weights):
            print(f"Loading weights from {args.weights}...")
            state_dict = torch.load(args.weights, map_location="cpu")
            if "params_ema" in state_dict:
                state_dict = state_dict["params_ema"]
            elif "params" in state_dict:
                state_dict = state_dict["params"]
            model.load_state_dict(state_dict, strict=True)
        else:
            print("Generating deterministic weights and saving to weights.pth...")
            torch.manual_seed(42)
            state_dict = {}
            for name, param in model.named_parameters():
                state_dict[name] = torch.randn_like(param) * 0.02
            model.load_state_dict(state_dict, strict=True)
            torch.save(state_dict, str(weight_file))

        onnx_file = out_dir / mname / "model.onnx"
        export_to_onnx(model, onnx_file, opset=args.opset)


if __name__ == "__main__":
    main()

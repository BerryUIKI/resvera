"""
Unit tests for export and parity toolchain integrity.
Validates fail-closed behavior:
- Missing --weights or --expected-sha256 argument fails with exit code != 0.
- Non-existent weights file fails with exit code != 0.
- SHA-256 mismatch fails with exit code != 0.
- Missing --onnx or expected hash arguments fails with exit code != 0.
- Normalization handles diverse checkpoint wrappers and strips module prefixes.
- Safe representative export and parity pipeline verification.
"""

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest

REPO_ROOT = Path(__file__).parent.parent.parent
EXPORT_SCRIPT = REPO_ROOT / "tools" / "export" / "export_realesrgan.py"
PARITY_SCRIPT = REPO_ROOT / "tools" / "parity" / "run_parity_test.py"

sys.path.insert(0, str(REPO_ROOT / "tools" / "export"))
from export_realesrgan import normalize_checkpoint_state_dict, get_sha256


class TestExportIntegrity(unittest.TestCase):
    def test_export_missing_weights_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXPORT_SCRIPT),
                    "--model",
                    "realesrgan-x4plus",
                    "--out-dir",
                    tmpdir,
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("required", result.stderr.lower())

    def test_export_missing_expected_sha256_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            dummy_weights = Path(tmpdir) / "weights.pth"
            dummy_weights.write_bytes(b"dummy")
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXPORT_SCRIPT),
                    "--model",
                    "realesrgan-x4plus",
                    "--weights",
                    str(dummy_weights),
                    "--out-dir",
                    tmpdir,
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("required", result.stderr.lower())
            self.assertIn("expected-sha256", result.stderr.lower())

    def test_export_nonexistent_weights_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            nonexistent = Path(tmpdir) / "missing_model.pth"
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXPORT_SCRIPT),
                    "--model",
                    "realesrgan-x4plus",
                    "--weights",
                    str(nonexistent),
                    "--expected-sha256",
                    "0000000000000000000000000000000000000000000000000000000000000000",
                    "--out-dir",
                    tmpdir,
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not found", result.stderr.lower())

    def test_export_sha256_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            dummy_weights = Path(tmpdir) / "dummy.pth"
            dummy_weights.write_bytes(b"dummy corrupted weights content")

            wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000"
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXPORT_SCRIPT),
                    "--model",
                    "realesrgan-x4plus",
                    "--weights",
                    str(dummy_weights),
                    "--expected-sha256",
                    wrong_hash,
                    "--out-dir",
                    tmpdir,
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("sha256 mismatch", result.stderr.lower())


class TestParityIntegrity(unittest.TestCase):
    def test_parity_missing_arguments_fails(self):
        result = subprocess.run(
            [sys.executable, str(PARITY_SCRIPT)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("required", result.stderr.lower())

    def test_parity_missing_hashes_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            dummy_weights = Path(tmpdir) / "weights.pth"
            dummy_weights.write_bytes(b"dummy")
            dummy_onnx = Path(tmpdir) / "model.onnx"
            dummy_onnx.write_bytes(b"dummy")

            result = subprocess.run(
                [
                    sys.executable,
                    str(PARITY_SCRIPT),
                    "--onnx",
                    str(dummy_onnx),
                    "--weights",
                    str(dummy_weights),
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("required", result.stderr.lower())

    def test_parity_nonexistent_onnx_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            dummy_weights = Path(tmpdir) / "weights.pth"
            dummy_weights.write_bytes(b"dummy")
            nonexistent_onnx = Path(tmpdir) / "nonexistent.onnx"

            result = subprocess.run(
                [
                    sys.executable,
                    str(PARITY_SCRIPT),
                    "--onnx",
                    str(nonexistent_onnx),
                    "--expected-onnx-sha256",
                    "0000000000000000000000000000000000000000000000000000000000000000",
                    "--weights",
                    str(dummy_weights),
                    "--expected-weights-sha256",
                    "0000000000000000000000000000000000000000000000000000000000000000",
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not found", result.stderr.lower())

    def test_parity_nonexistent_weights_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            dummy_onnx = Path(tmpdir) / "model.onnx"
            dummy_onnx.write_bytes(b"dummy")
            nonexistent_weights = Path(tmpdir) / "nonexistent.pth"

            result = subprocess.run(
                [
                    sys.executable,
                    str(PARITY_SCRIPT),
                    "--onnx",
                    str(dummy_onnx),
                    "--expected-onnx-sha256",
                    "0000000000000000000000000000000000000000000000000000000000000000",
                    "--weights",
                    str(nonexistent_weights),
                    "--expected-weights-sha256",
                    "0000000000000000000000000000000000000000000000000000000000000000",
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not found", result.stderr.lower())

    def test_parity_hash_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            dummy_onnx = Path(tmpdir) / "model.onnx"
            dummy_onnx.write_bytes(b"dummy onnx content")
            dummy_weights = Path(tmpdir) / "weights.pth"
            dummy_weights.write_bytes(b"dummy weights content")

            onnx_hash = get_sha256(dummy_onnx)
            wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000"

            result = subprocess.run(
                [
                    sys.executable,
                    str(PARITY_SCRIPT),
                    "--onnx",
                    str(dummy_onnx),
                    "--expected-onnx-sha256",
                    onnx_hash,
                    "--weights",
                    str(dummy_weights),
                    "--expected-weights-sha256",
                    wrong_hash,
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("sha256 mismatch", result.stderr.lower())


class TestCheckpointNormalization(unittest.TestCase):
    def test_params_ema_wrapper(self):
        raw = {"params_ema": {"conv1.weight": 123}}
        normalized = normalize_checkpoint_state_dict(raw)
        self.assertEqual(normalized, {"conv1.weight": 123})

    def test_params_wrapper(self):
        raw = {"params": {"conv2.weight": 456}}
        normalized = normalize_checkpoint_state_dict(raw)
        self.assertEqual(normalized, {"conv2.weight": 456})

    def test_module_prefix_stripped(self):
        raw = {"params_ema": {"module.conv1.weight": 789}}
        normalized = normalize_checkpoint_state_dict(raw)
        self.assertEqual(normalized, {"conv1.weight": 789})

    def test_non_dict_rejected(self):
        with self.assertRaises(ValueError):
            normalize_checkpoint_state_dict([1, 2, 3])


class TestRepresentativeExportAndParityGate(unittest.TestCase):
    def test_representative_pipeline_if_dependencies_available(self):
        try:
            import torch
            import onnx
            import onnxruntime
            from arch_rrdb import RRDBNet
        except ImportError:
            self.skipTest("ML dependencies (torch, onnx, onnxruntime) not installed in current environment")

        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            # Create a small valid 6-block RRDBNet checkpoint (RealESRGAN_x4plus_anime architecture)
            model = RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=6, num_grow_ch=32, scale=4)
            model.eval()

            # Save state dict wrapped inside params_ema
            weights_file = tmp_path / "test_anime_weights.pth"
            torch.save({"params_ema": model.state_dict()}, str(weights_file))

            weights_hash = get_sha256(weights_file)

            # 1. Run export_realesrgan.py
            export_cmd = [
                sys.executable,
                str(EXPORT_SCRIPT),
                "--model",
                "realesrgan-x4plus-anime",
                "--weights",
                str(weights_file),
                "--expected-sha256",
                weights_hash,
                "--out-dir",
                str(tmp_path),
            ]
            exp_res = subprocess.run(export_cmd, capture_output=True, text=True)
            self.assertEqual(
                exp_res.returncode,
                0,
                f"Export failed:\nSTDOUT:\n{exp_res.stdout}\nSTDERR:\n{exp_res.stderr}",
            )

            onnx_file = tmp_path / "realesrgan-x4plus-anime.onnx"
            self.assertTrue(onnx_file.is_file(), "Exported ONNX file must exist")
            onnx_hash = get_sha256(onnx_file)

            # 2. Run run_parity_test.py with report generation
            report_file = tmp_path / "parity_report.json"
            parity_cmd = [
                sys.executable,
                str(PARITY_SCRIPT),
                "--model",
                "realesrgan-x4plus-anime",
                "--onnx",
                str(onnx_file),
                "--expected-onnx-sha256",
                onnx_hash,
                "--weights",
                str(weights_file),
                "--expected-weights-sha256",
                weights_hash,
                "--num-blocks",
                "6",
                "--seed",
                "1337",
                "--report-path",
                str(report_file),
            ]
            parity_res = subprocess.run(parity_cmd, capture_output=True, text=True)
            self.assertEqual(
                parity_res.returncode,
                0,
                f"Parity run failed:\nSTDOUT:\n{parity_res.stdout}\nSTDERR:\n{parity_res.stderr}",
            )

            # 3. Verify parity report
            self.assertTrue(report_file.is_file(), "Parity report JSON must exist")
            report_data = json.loads(report_file.read_text(encoding="utf-8"))
            self.assertTrue(report_data["all_passed"], "All fixtures must pass parity thresholds")
            self.assertIn("environment", report_data)
            self.assertEqual(report_data["inputs"]["weights_sha256"], weights_hash)
            self.assertEqual(report_data["inputs"]["onnx_sha256"], onnx_hash)
            self.assertGreater(len(report_data["fixtures"]), 0)


if __name__ == "__main__":
    unittest.main()

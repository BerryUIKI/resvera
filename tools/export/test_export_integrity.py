"""
Unit tests for export and parity toolchain integrity.
Validates fail-closed behavior:
- Missing --weights argument fails with exit code != 0.
- Non-existent weights file fails with exit code != 0.
- SHA-256 mismatch fails with exit code != 0.
- Missing --onnx argument fails with exit code != 0.
"""

import subprocess
import sys
import tempfile
from pathlib import Path
import unittest

REPO_ROOT = Path(__file__).parent.parent.parent
EXPORT_SCRIPT = REPO_ROOT / "tools" / "export" / "export_realesrgan.py"
PARITY_SCRIPT = REPO_ROOT / "tools" / "parity" / "run_parity_test.py"


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
                    "--weights",
                    str(dummy_weights),
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
                    "--weights",
                    str(nonexistent_weights),
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not found", result.stderr.lower())


if __name__ == "__main__":
    unittest.main()

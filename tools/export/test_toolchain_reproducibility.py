"""
Toolchain and Environment Reproducibility Verification Tests.
Ensures that all development and CI environments are deterministically pinned:
- rust-toolchain.toml specifies an exact version (not 'stable' or 'nightly').
- .python-version specifies an exact 3-part version (e.g. 3.12.7).
- .node-version specifies an exact 3-part version (e.g. 22.13.0).
- package.json pins packageManager and engines.
- tools/export/requirements.txt uses strict '==' pins without wildcards or ranges.
- tools/export/requirements-lock.txt exists and is non-empty.
- Export tool execution is deterministic: identical input weights yield identical ONNX SHA-256.
"""

import re
import json
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest

REPO_ROOT = Path(__file__).parent.parent.parent
EXPORT_SCRIPT = REPO_ROOT / "tools" / "export" / "export_realesrgan.py"


class TestToolchainPins(unittest.TestCase):
    def test_rust_toolchain_is_strictly_pinned(self):
        toolchain_file = REPO_ROOT / "rust-toolchain.toml"
        self.assertTrue(toolchain_file.is_file(), "rust-toolchain.toml must exist")
        content = toolchain_file.read_text(encoding="utf-8")
        match = re.search(r'channel\s*=\s*"([^"]+)"', content)
        self.assertIsNotNone(match, "rust-toolchain.toml must declare channel")
        channel = match.group(1).strip()
        self.assertNotIn(channel, ["stable", "beta", "nightly"], "Rust channel must not be a floating channel")
        self.assertRegex(channel, r"^\d+\.\d+\.\d+$", f"Rust channel '{channel}' must be exact 3-part SemVer")

    def test_python_version_is_strictly_pinned(self):
        py_file = REPO_ROOT / ".python-version"
        self.assertTrue(py_file.is_file(), ".python-version must exist")
        content = py_file.read_text(encoding="utf-8").strip()
        self.assertRegex(content, r"^\d+\.\d+\.\d+$", f".python-version '{content}' must be exact 3-part SemVer")

    def test_node_version_is_strictly_pinned(self):
        node_file = REPO_ROOT / ".node-version"
        self.assertTrue(node_file.is_file(), ".node-version must exist")
        content = node_file.read_text(encoding="utf-8").strip()
        self.assertRegex(content, r"^\d+\.\d+\.\d+$", f".node-version '{content}' must be exact 3-part SemVer")

    def test_package_json_engines_and_package_manager_pinned(self):
        pkg_file = REPO_ROOT / "package.json"
        self.assertTrue(pkg_file.is_file(), "package.json must exist")
        data = json.loads(pkg_file.read_text(encoding="utf-8"))
        self.assertIn("packageManager", data, "package.json must define packageManager")
        self.assertRegex(data["packageManager"], r"^pnpm@\d+\.\d+\.\d+$", "packageManager must be exact pnpm SemVer")
        self.assertIn("engines", data, "package.json must define engines")
        self.assertIn("node", data["engines"])
        self.assertIn("pnpm", data["engines"])

    def test_export_requirements_strictly_pinned(self):
        req_file = REPO_ROOT / "tools" / "export" / "requirements.txt"
        self.assertTrue(req_file.is_file(), "requirements.txt must exist")
        lines = [line.strip() for line in req_file.read_text(encoding="utf-8").splitlines()]
        pkg_lines = [line for line in lines if line and not line.startswith("#")]
        self.assertGreater(len(pkg_lines), 0, "requirements.txt must have dependencies")
        for line in pkg_lines:
            self.assertIn("==", line, f"Requirement '{line}' must use strict '==' pinning")
            self.assertNotIn(">=", line, f"Requirement '{line}' must not use '>=' range")
            self.assertNotIn("<=", line, f"Requirement '{line}' must not use '<=' range")
            self.assertNotIn("~=", line, f"Requirement '{line}' must not use '~=' range")

    def test_export_requirements_lock_exists(self):
        lock_file = REPO_ROOT / "tools" / "export" / "requirements-lock.txt"
        self.assertTrue(lock_file.is_file(), "requirements-lock.txt must exist")
        content = lock_file.read_text(encoding="utf-8").strip()
        self.assertGreater(len(content), 10, "requirements-lock.txt must not be empty")

    def test_export_determinism_if_dependencies_available(self):
        try:
            import torch
            import onnx
            import onnxruntime
            from arch_rrdb import RRDBNet
            from export_realesrgan import get_sha256
        except ImportError:
            self.skipTest("ML dependencies not available in current environment")

        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            model = RRDBNet(num_in_ch=3, num_out_ch=3, num_feat=64, num_block=6, num_grow_ch=32, scale=4)
            model.eval()

            weights_file = tmp_path / "model_weights.pth"
            torch.save({"params_ema": model.state_dict()}, str(weights_file))
            weights_hash = get_sha256(weights_file)

            out_dir_1 = tmp_path / "run1"
            out_dir_2 = tmp_path / "run2"

            cmd1 = [
                sys.executable,
                str(EXPORT_SCRIPT),
                "--model", "realesrgan-x4plus-anime",
                "--weights", str(weights_file),
                "--expected-sha256", weights_hash,
                "--out-dir", str(out_dir_1),
            ]
            cmd2 = [
                sys.executable,
                str(EXPORT_SCRIPT),
                "--model", "realesrgan-x4plus-anime",
                "--weights", str(weights_file),
                "--expected-sha256", weights_hash,
                "--out-dir", str(out_dir_2),
            ]

            res1 = subprocess.run(cmd1, capture_output=True, text=True)
            self.assertEqual(res1.returncode, 0)
            res2 = subprocess.run(cmd2, capture_output=True, text=True)
            self.assertEqual(res2.returncode, 0)

            onnx1 = out_dir_1 / "realesrgan-x4plus-anime.onnx"
            onnx2 = out_dir_2 / "realesrgan-x4plus-anime.onnx"

            hash1 = get_sha256(onnx1)
            hash2 = get_sha256(onnx2)
            self.assertEqual(hash1, hash2, "Repeated export of identical weights must produce identical SHA-256")


if __name__ == "__main__":
    unittest.main()

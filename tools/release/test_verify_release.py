#!/usr/bin/env python3
"""
Unit tests for Resvera release verification tool (tools/release/verify_release_artifacts.py).
"""

import base64
import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path

from tools.release.verify_release_artifacts import (
    compute_sha256,
    generate_sha256sums,
    verify_all_release_artifacts,
    verify_sha256sums,
    verify_sbom,
    verify_updater_manifest,
)


class TestReleaseVerification(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp(prefix="resvera_rel_test_")
        self.dist_path = Path(self.test_dir)

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_compute_and_verify_sha256sums(self):
        file1 = self.dist_path / "resvera-setup.exe"
        file1.write_bytes(b"windows installer binary content")
        file2 = self.dist_path / "resvera.dmg"
        file2.write_bytes(b"macos disk image binary content")

        sums_file = generate_sha256sums(self.dist_path)
        self.assertTrue(sums_file.exists())

        ok, errors = verify_sha256sums(self.dist_path, sums_file)
        self.assertTrue(ok)
        self.assertEqual(len(errors), 0)

    def test_sha256sums_detects_tampering(self):
        file1 = self.dist_path / "resvera-setup.exe"
        file1.write_bytes(b"windows installer original content")
        sums_file = generate_sha256sums(self.dist_path)

        # Tamper with file
        file1.write_bytes(b"tampered malicious content")

        ok, errors = verify_sha256sums(self.dist_path, sums_file)
        self.assertFalse(ok)
        self.assertTrue(any("Checksum mismatch" in e for e in errors))

    def test_verify_sbom_valid_and_invalid(self):
        valid_sbom = self.dist_path / "sbom-rust.cdx.json"
        valid_sbom.write_text(
            json.dumps({
                "bomFormat": "CycloneDX",
                "specVersion": "1.5",
                "version": 1,
                "components": [
                    {"name": "resvera-core", "version": "0.1.0", "type": "library"},
                    {"name": "ort", "version": "2.0.0-rc.13", "type": "library"},
                ],
            }),
            encoding="utf-8",
        )

        ok, errors = verify_sbom(valid_sbom)
        self.assertTrue(ok)
        self.assertEqual(len(errors), 0)

        # Invalid format
        invalid_sbom = self.dist_path / "sbom-bad.cdx.json"
        invalid_sbom.write_text(
            json.dumps({
                "bomFormat": "SPDX",
                "specVersion": "1.5",
                "components": [],
            }),
            encoding="utf-8",
        )
        ok, errors = verify_sbom(invalid_sbom)
        self.assertFalse(ok)
        self.assertTrue(any("Invalid bomFormat" in e for e in errors))

    def test_verify_updater_manifest_valid_and_invalid(self):
        manifest_file = self.dist_path / "latest.json"
        valid_sig = base64.b64encode(b"A" * 64).decode("ascii")

        manifest_file.write_text(
            json.dumps({
                "version": "v0.1.0",
                "notes": "Initial release",
                "pub_date": "2026-09-22T00:00:00Z",
                "platforms": {
                    "windows-x86_64": {
                        "signature": valid_sig,
                        "url": "https://github.com/BerryUIKI/resvera/releases/download/v0.1.0/resvera-setup.exe.zip",
                    },
                    "darwin-aarch64": {
                        "signature": valid_sig,
                        "url": "https://github.com/BerryUIKI/resvera/releases/download/v0.1.0/resvera.app.tar.gz",
                    },
                },
            }),
            encoding="utf-8",
        )

        ok, errors = verify_updater_manifest(manifest_file, expected_version="0.1.0")
        self.assertTrue(ok)
        self.assertEqual(len(errors), 0)

        # Version mismatch
        ok, errors = verify_updater_manifest(manifest_file, expected_version="0.2.0")
        self.assertFalse(ok)
        self.assertTrue(any("Version mismatch" in e for e in errors))

        # Malformed signature
        bad_manifest = self.dist_path / "bad_latest.json"
        short_sig = base64.b64encode(b"short").decode("ascii")
        bad_manifest.write_text(
            json.dumps({
                "version": "v0.1.0",
                "pub_date": "2026-09-22T00:00:00Z",
                "platforms": {
                    "windows-x86_64": {
                        "signature": short_sig,
                        "url": "https://example.com/app.zip",
                    }
                },
            }),
            encoding="utf-8",
        )
        ok, errors = verify_updater_manifest(bad_manifest)
        self.assertFalse(ok)
        self.assertTrue(any("signature is too short" in e for e in errors))

    def test_verify_all_release_artifacts_e2e(self):
        # Create artifacts
        (self.dist_path / "resvera.exe").write_bytes(b"win-bin")
        (self.dist_path / "sbom-rust.cdx.json").write_text(
            json.dumps({
                "bomFormat": "CycloneDX",
                "specVersion": "1.5",
                "components": [{"name": "resvera-core", "version": "0.1.0"}],
            }),
            encoding="utf-8",
        )
        sig = base64.b64encode(b"X" * 64).decode("ascii")
        (self.dist_path / "latest.json").write_text(
            json.dumps({
                "version": "0.1.0",
                "pub_date": "2026-09-22T00:00:00Z",
                "platforms": {
                    "windows-x86_64": {
                        "signature": sig,
                        "url": "https://example.com/download.zip",
                    }
                },
            }),
            encoding="utf-8",
        )

        generate_sha256sums(self.dist_path)

        ok, errors = verify_all_release_artifacts(
            self.dist_path,
            expected_version="0.1.0",
            require_sbom=True,
            require_updater=True,
        )
        self.assertTrue(ok)
        self.assertEqual(len(errors), 0)


if __name__ == "__main__":
    unittest.main()

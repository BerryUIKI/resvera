#!/usr/bin/env python3
"""
Unit tests for the model artifact publishing tool
(tools/release/publish_model_artifacts.py).

The publish step itself needs network access and a production key, so these
tests cover the offline guarantees: the tool must refuse to proceed on a
tampered artifact, a malformed signing key, or a missing artifact.
"""

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from tools.release.publish_model_artifacts import (
    ARTIFACTS,
    CATALOG_PATH,
    assert_no_placeholder_key,
    sha256_file,
    verify_artifact,
)


class TestPublishGuards(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.mkdtemp()
        self.temp_path = Path(self.temp)
        with open(CATALOG_PATH, encoding="utf-8") as handle:
            self.catalog = json.load(handle)
        self.entry = next(e for e in self.catalog["models"] if e["id"] == "realesrgan-x4plus")

    def tearDown(self):
        shutil.rmtree(self.temp, ignore_errors=True)

    def _write_artifact(self, content: bytes) -> Path:
        path = self.temp_path / ARTIFACTS["realesrgan-x4plus"]
        path.write_bytes(content)
        return path

    def test_verify_artifact_accepts_matching_bytes(self):
        artifact = self.temp_path / ARTIFACTS["realesrgan-x4plus"]
        artifact.write_bytes(b"payload")
        entry = {
            "id": "realesrgan-x4plus",
            "sha256": sha256_file(artifact),
            "size_bytes": artifact.stat().st_size,
        }
        # Must not raise.
        verify_artifact(entry, artifact)

    def test_verify_artifact_rejects_digest_mismatch(self):
        artifact = self._write_artifact(b"payload")
        entry = dict(self.entry)
        entry["size_bytes"] = artifact.stat().st_size
        with self.assertRaises(SystemExit) as ctx:
            verify_artifact(entry, artifact)
        self.assertIn("digest mismatch", str(ctx.exception))

    def test_verify_artifact_rejects_size_mismatch(self):
        artifact = self._write_artifact(b"payload")
        entry = {
            "id": "realesrgan-x4plus",
            "sha256": sha256_file(artifact),
            "size_bytes": 999_999,
        }
        with self.assertRaises(SystemExit) as ctx:
            verify_artifact(entry, artifact)
        self.assertIn("size mismatch", str(ctx.exception))

    def test_verify_artifact_rejects_missing_file(self):
        missing = self.temp_path / "nope.onnx"
        with self.assertRaises(SystemExit) as ctx:
            verify_artifact(self.entry, missing)
        self.assertIn("missing artifact", str(ctx.exception))

    def test_verify_artifact_rejects_non_onnx(self):
        artifact = self.temp_path / "model.pth"
        artifact.write_bytes(b"payload")
        entry = {
            "id": "realesrgan-x4plus",
            "sha256": sha256_file(artifact),
            "size_bytes": artifact.stat().st_size,
        }
        with self.assertRaises(SystemExit) as ctx:
            verify_artifact(entry, artifact)
        self.assertIn(".onnx", str(ctx.exception))


class TestSigningKeyGuard(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.mkdtemp()
        self.temp_path = Path(self.temp)

    def tearDown(self):
        shutil.rmtree(self.temp, ignore_errors=True)

    def test_accepts_64_hex_characters(self):
        key = self.temp_path / "key.hex"
        key.write_text("a" * 64, encoding="utf-8")
        assert_no_placeholder_key(key)  # must not raise

    def test_rejects_missing_file(self):
        with self.assertRaises(SystemExit) as ctx:
            assert_no_placeholder_key(self.temp_path / "absent.hex")
        self.assertIn("not found", str(ctx.exception))

    def test_rejects_wrong_length(self):
        key = self.temp_path / "short.hex"
        key.write_text("abcd", encoding="utf-8")
        with self.assertRaises(SystemExit) as ctx:
            assert_no_placeholder_key(key)
        self.assertIn("64 hex", str(ctx.exception))

    def test_rejects_non_hex(self):
        key = self.temp_path / "bad.hex"
        key.write_text("z" * 64, encoding="utf-8")
        with self.assertRaises(SystemExit) as ctx:
            assert_no_placeholder_key(key)
        self.assertIn("64 hex", str(ctx.exception))


class TestCatalogWiring(unittest.TestCase):
    def test_every_publishable_model_is_in_the_catalog(self):
        with open(CATALOG_PATH, encoding="utf-8") as handle:
            catalog = json.load(handle)
        catalog_ids = {entry["id"] for entry in catalog["models"]}
        for model_id in ARTIFACTS:
            self.assertIn(
                model_id,
                catalog_ids,
                f"{model_id} is listed for publishing but is absent from the catalog",
            )


if __name__ == "__main__":
    unittest.main()

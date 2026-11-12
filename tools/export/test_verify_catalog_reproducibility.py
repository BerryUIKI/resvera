#!/usr/bin/env python3
"""
Tests for tools/export/verify_catalog_reproducibility.py.

These cover the offline guarantees that CI relies on: a catalog digest with no
committed evidence, incomplete provenance, or a placeholder checkpoint hash must
all be rejected. The full export path needs network and torch, so it is out of
scope here and exercised by `--export` in the documented manual procedure.
"""

import json
import sys
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import tools.export.verify_catalog_reproducibility as verify


def make_entry(**overrides):
    manifest = {
        "provenance": {
            "upstream_repository": "https://github.com/xinntao/Real-ESRGAN",
            "upstream_revision": "v0.3.0",
            "source_weight_name": "RealESRGAN_x4plus.pth",
            "source_weight_sha256": "4fa0d38905f75ac06eb49a7951b426670021be3018265fd191d2125df9d682f1",
            "export_recipe": "official-onnx-export",
        },
        "artifacts": [
            {
                "path": "artifacts/model.onnx",
                "size_bytes": 67051643,
                "sha256": "1d276713aa9cd034c9d973580f773954ca988fb9b0e040e370564e353f1c30d2",
            }
        ],
    }
    entry = {
        "id": "realesrgan-x4plus",
        "sha256": "1d276713aa9cd034c9d973580f773954ca988fb9b0e040e370564e353f1c30d2",
        "size_bytes": 67051643,
        "manifest_template": json.dumps(manifest),
    }
    entry.update(overrides)
    return entry


EVIDENCE = {"1d276713aa9cd034c9d973580f773954ca988fb9b0e040e370564e353f1c30d2": ["report.md"]}


class TestDigestHelpers(unittest.TestCase):
    def test_is_sha256(self):
        self.assertTrue(verify.is_sha256("a" * 64))
        self.assertFalse(verify.is_sha256("a" * 63))
        self.assertFalse(verify.is_sha256("g" * 64))
        self.assertFalse(verify.is_sha256(None))

    def test_is_placeholder(self):
        self.assertTrue(verify.is_placeholder("0" * 64))
        self.assertTrue(verify.is_placeholder("f" * 64))
        self.assertFalse(verify.is_placeholder("1" * 64))


class TestEntryChecks(unittest.TestCase):
    def test_accepts_a_complete_entry(self):
        errors = []
        verify.check_entry(make_entry(), EVIDENCE, errors)
        self.assertEqual(errors, [])

    def test_rejects_digest_absent_from_evidence(self):
        errors = []
        verify.check_entry(make_entry(), {}, errors)
        self.assertTrue(any("appears in no committed evidence" in e for e in errors))

    def test_rejects_placeholder_checkpoint_hash(self):
        entry = make_entry()
        manifest = json.loads(entry["manifest_template"])
        manifest["provenance"]["source_weight_sha256"] = "0" * 64
        entry["manifest_template"] = json.dumps(manifest)

        errors = []
        verify.check_entry(entry, EVIDENCE, errors)
        self.assertTrue(any("placeholder" in e for e in errors))

    def test_rejects_empty_provenance_field(self):
        entry = make_entry()
        manifest = json.loads(entry["manifest_template"])
        manifest["provenance"]["upstream_revision"] = ""
        entry["manifest_template"] = json.dumps(manifest)

        errors = []
        verify.check_entry(entry, EVIDENCE, errors)
        self.assertTrue(any("upstream_revision is empty" in e for e in errors))

    def test_rejects_manifest_digest_disagreement(self):
        entry = make_entry()
        manifest = json.loads(entry["manifest_template"])
        manifest["artifacts"][0]["sha256"] = "f" * 64
        entry["manifest_template"] = json.dumps(manifest)

        errors = []
        verify.check_entry(entry, EVIDENCE, errors)
        self.assertTrue(any("disagrees with the catalog" in e for e in errors))

    def test_rejects_malformed_manifest_template(self):
        errors = []
        verify.check_entry(make_entry(manifest_template="{not json"), EVIDENCE, errors)
        self.assertTrue(any("not valid JSON" in e for e in errors))

    def test_rejects_non_positive_size(self):
        errors = []
        verify.check_entry(make_entry(size_bytes=0), EVIDENCE, errors)
        self.assertTrue(any("positive integer" in e for e in errors))


class TestMainOffline(unittest.TestCase):
    def setUp(self):
        self.temp = Path(tempfile.mkdtemp())

    def tearDown(self):
        shutil.rmtree(self.temp, ignore_errors=True)

    def _run(self, catalog, evidence):
        catalog_path = self.temp / "production.json"
        catalog_path.write_text(json.dumps(catalog), encoding="utf-8")
        with mock.patch.object(verify, "CATALOG_PATH", catalog_path), \
             mock.patch.object(verify, "collect_evidence_digests", return_value=evidence), \
             mock.patch.object(sys, "argv", ["x"]):
            return verify.main()

    def test_passes_for_a_consistent_catalog(self):
        catalog = {"models": [make_entry()]}
        self.assertEqual(self._run(catalog, EVIDENCE), 0)

    def test_fails_when_evidence_missing(self):
        catalog = {"models": [make_entry()]}
        self.assertEqual(self._run(catalog, {}), 1)

    def test_fails_on_mismatched_local_artifact(self):
        catalog = {"models": [make_entry()]}
        artifacts = self.temp / "artifacts"
        artifacts.mkdir()
        (artifacts / "realesrgan-x4plus.onnx").write_bytes(b"wrong bytes")

        catalog_path = self.temp / "production.json"
        catalog_path.write_text(json.dumps(catalog), encoding="utf-8")
        with mock.patch.object(verify, "CATALOG_PATH", catalog_path), \
             mock.patch.object(verify, "collect_evidence_digests", return_value=EVIDENCE), \
             mock.patch.object(sys, "argv", ["x", "--artifacts-dir", str(artifacts)]):
            self.assertEqual(verify.main(), 1)


class TestRealCatalog(unittest.TestCase):
    """The committed catalog must pass the same check CI runs."""

    def test_committed_catalog_verifies_offline(self):
        with mock.patch.object(sys, "argv", ["x"]):
            self.assertEqual(verify.main(), 0)


if __name__ == "__main__":
    unittest.main()

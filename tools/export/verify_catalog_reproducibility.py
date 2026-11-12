#!/usr/bin/env python3
"""
tools/export/verify_catalog_reproducibility.py

Checks that the digests pinned in the production catalog are actually
reproducible from the recorded upstream checkpoints, and that the committed
evidence supports that claim.

Two modes:

  --offline (default)
      No network. Verifies that every catalog entry carries complete,
      consistent provenance and that a parity/export evidence record exists for
      the exact ONNX digest it pins. This is what CI runs.

  --export
      Also downloads each upstream checkpoint, confirms its hash matches
      `provenance.source_weight_sha256`, re-exports with the pinned toolchain,
      and asserts the result is byte-identical to the catalog digest. Requires
      torch/onnx installed and network access; it is NOT part of CI.

Why the split: a full export needs ~85 MB of checkpoints, several minutes of
CPU, and pulls from the public internet on every run. The offline check is
still meaningful, because it rejects the failure this exists to prevent — a
catalog carrying a digest that no committed artifact or export record backs.
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
CATALOG_PATH = REPO_ROOT / "crates" / "resvera-models" / "src" / "catalogs" / "production.json"
EVIDENCE_DIR = REPO_ROOT / "docs" / "research"


def sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def is_sha256(value: str) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(
        c in "0123456789abcdefABCDEF" for c in value
    )


def is_placeholder(value: str) -> bool:
    lowered = value.lower()
    return all(c == "0" for c in lowered) or all(c == "f" for c in lowered)


def load_catalog() -> dict:
    with open(CATALOG_PATH, encoding="utf-8") as handle:
        return json.load(handle)


def collect_evidence_digests() -> dict:
    """
    ONNX digests mentioned anywhere under docs/research. Parsed generously so
    the check does not depend on one report's exact formatting.
    """
    found = {}
    for path in EVIDENCE_DIR.rglob("*.md"):
        text = path.read_text(encoding="utf-8", errors="replace")
        for token in text.replace("`", " ").split():
            candidate = token.strip().rstrip(".,;:)")
            if is_sha256(candidate):
                found.setdefault(candidate.lower(), []).append(path.name)
    return found


def check_entry(entry: dict, evidence: dict, errors: list) -> None:
    model_id = entry["id"]
    onnx_sha = entry.get("sha256", "")
    size = entry.get("size_bytes")

    if not is_sha256(onnx_sha):
        errors.append(f"{model_id}: sha256 is not a 64-character hex digest")
        return
    if not isinstance(size, int) or size <= 0:
        errors.append(f"{model_id}: size_bytes must be a positive integer")

    template = entry.get("manifest_template")
    if not template:
        errors.append(f"{model_id}: missing manifest_template")
        return

    try:
        manifest = json.loads(template)
    except json.JSONDecodeError as exc:
        errors.append(f"{model_id}: manifest_template is not valid JSON: {exc}")
        return

    provenance = manifest.get("provenance", {})
    pth_sha = provenance.get("source_weight_sha256", "")
    if not is_sha256(pth_sha):
        errors.append(f"{model_id}: provenance.source_weight_sha256 is not a valid digest")
    elif is_placeholder(pth_sha):
        errors.append(
            f"{model_id}: provenance.source_weight_sha256 is a placeholder; the artifact "
            "cannot be traced to a specific checkpoint"
        )

    for field in ("upstream_repository", "upstream_revision", "source_weight_name", "export_recipe"):
        if not str(provenance.get(field, "")).strip():
            errors.append(f"{model_id}: provenance.{field} is empty")

    artifact = manifest.get("artifacts", [{}])[0]
    if artifact.get("sha256", "").lower() != onnx_sha.lower():
        errors.append(f"{model_id}: manifest artifact digest disagrees with the catalog entry")
    if artifact.get("size_bytes") != size:
        errors.append(f"{model_id}: manifest artifact size disagrees with the catalog entry")

    # The central claim: this digest must appear in committed evidence, so it is
    # not an unverifiable constant.
    if onnx_sha.lower() not in evidence:
        errors.append(
            f"{model_id}: ONNX digest {onnx_sha[:16]}... appears in no committed evidence "
            f"under {EVIDENCE_DIR.relative_to(REPO_ROOT)}; the catalog cannot be reproduced "
            "from the repository"
        )


def run_export_check(entry: dict, artifacts_dir: Path, errors: list) -> None:
    """Full reproduction: fetch the checkpoint, export, compare bytes."""
    import subprocess
    import tempfile
    import urllib.request

    manifest = json.loads(entry["manifest_template"])
    provenance = manifest["provenance"]
    model_id = entry["id"]

    repo = provenance["upstream_repository"].rstrip("/")
    revision = provenance["upstream_revision"]
    weight_name = provenance["source_weight_name"]
    if "xinntao/Real-ESRGAN" not in repo:
        errors.append(
            f"{model_id}: automatic export is only wired for Real-ESRGAN checkpoints; "
            f"found {repo}. Extend run_export_check before enabling --export for this model."
        )
        return

    release_tag = {"realesrgan-x4plus": "v0.1.0", "realesrgan-x4plus-anime": "v0.2.2.4"}.get(model_id)
    if release_tag is None:
        errors.append(f"{model_id}: no known upstream release tag for this checkpoint")
        return

    url = f"{repo}/releases/download/{release_tag}/{weight_name}"
    with tempfile.TemporaryDirectory() as tmp:
        checkpoint = Path(tmp) / weight_name
        print(f"  downloading {url}")
        try:
            urllib.request.urlretrieve(url, checkpoint)
        except Exception as exc:  # noqa: BLE001 - reported as an error, not raised
            errors.append(f"{model_id}: could not download checkpoint: {exc}")
            return

        actual_pth = sha256_file(checkpoint)
        if actual_pth != provenance["source_weight_sha256"].lower():
            errors.append(
                f"{model_id}: checkpoint digest mismatch\n"
                f"  catalog: {provenance['source_weight_sha256']}\n"
                f"  fetched: {actual_pth}"
            )
            return

        out_dir = Path(tmp) / "out"
        command = [
            sys.executable,
            str(REPO_ROOT / "tools" / "export" / "export_realesrgan.py"),
            "--model", model_id,
            "--weights", str(checkpoint),
            "--expected-sha256", provenance["source_weight_sha256"],
            "--out-dir", str(out_dir),
        ]
        print(f"  exporting with {Path(sys.executable).name}")
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode != 0:
            errors.append(f"{model_id}: export failed\n{result.stderr[-800:]}")
            return

        produced = out_dir / f"{model_id}.onnx"
        produced_sha = sha256_file(produced)
        produced_size = produced.stat().st_size
        if produced_sha != entry["sha256"].lower():
            errors.append(
                f"{model_id}: re-export is not byte-identical to the catalog\n"
                f"  catalog:    {entry['sha256']}\n"
                f"  re-exported: {produced_sha}\n"
                "  If this is a platform difference, record it explicitly rather than "
                "loosening this check."
            )
        elif produced_size != entry["size_bytes"]:
            errors.append(f"{model_id}: re-export size {produced_size} != catalog {entry['size_bytes']}")
        else:
            print(f"  {model_id}: reproduced {produced_sha[:16]}...")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    parser.add_argument(
        "--export",
        action="store_true",
        help="Also fetch checkpoints and re-export (network + torch/onnx required)",
    )
    parser.add_argument(
        "--artifacts-dir",
        type=Path,
        default=REPO_ROOT / "artifacts" / "exports",
        help="Where local .onnx artifacts live, if present",
    )
    args = parser.parse_args()

    catalog = load_catalog()
    evidence = collect_evidence_digests()
    errors: list = []

    try:
        catalog_label = CATALOG_PATH.relative_to(REPO_ROOT)
    except ValueError:
        catalog_label = CATALOG_PATH
    print(f"catalog:  {catalog_label}")
    print(f"evidence: {len(evidence)} distinct digest(s) recorded under docs/research")
    print(f"models:   {len(catalog.get('models', []))}")

    for entry in catalog.get("models", []):
        check_entry(entry, evidence, errors)

    # If a local artifact exists, it must match the catalog. This is a cheap and
    # direct check when the developer has run an export.
    for entry in catalog.get("models", []):
        local = args.artifacts_dir / f"{entry['id']}.onnx"
        if local.is_file():
            actual = sha256_file(local)
            if actual != entry["sha256"].lower():
                errors.append(
                    f"{entry['id']}: local artifact {local.name} does not match the catalog\n"
                    f"  catalog: {entry['sha256']}\n"
                    f"  local:   {actual}"
                )
            else:
                print(f"{entry['id']}: local artifact matches catalog")

    if args.export:
        print("\nfull reproduction (network + torch/onnx):")
        try:
            import torch  # noqa: F401
            import onnx  # noqa: F401
        except ImportError as exc:
            errors.append(f"--export requires torch and onnx: {exc}")
        else:
            for entry in catalog.get("models", []):
                run_export_check(entry, args.artifacts_dir, errors)

    if errors:
        print("\nFAILED:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    mode = "offline + full reproduction" if args.export else "offline"
    print(f"\nOK: catalog digests are backed by committed evidence ({mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

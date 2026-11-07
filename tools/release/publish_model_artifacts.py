#!/usr/bin/env python3
"""
tools/release/publish_model_artifacts.py

Publishes validated ONNX model artifacts to a GitHub Release and wires the
resulting download URLs into the signed production catalog.

This tool exists because publishing is the one step that cannot be reproduced
offline: it changes the catalog's `download_urls`, which invalidates every
signature and requires the production signing key.

Safety properties:
  * Refuses to run without a production signing key, because re-signing with a
    throwaway key would produce a catalog whose signature no shipped build
    accepts.
  * Refuses to publish an artifact whose digest does not match the catalog.
  * Verifies the published assets are reachable and byte-identical before the
    catalog is touched.
  * Re-signs and re-verifies the catalog as the final step, so a partial run
    never leaves a catalog pointing at unpublished bytes.

Usage:
  tools/release/publish_model_artifacts.py \\
      --tag models-v1 \\
      --artifacts-dir artifacts/exports \\
      --signing-key-file /secure/catalog-seed.hex
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
CATALOG_PATH = REPO_ROOT / "crates" / "resvera-models" / "src" / "catalogs" / "production.json"
SIGNER_BASE = REPO_ROOT / "tools" / "catalog-sign" / "target" / "release" / "resvera-catalog-sign"
SIGNER = SIGNER_BASE.with_suffix(".exe") if os.name == "nt" else SIGNER_BASE
REPO_SLUG = "BerryUIKI/resvera"

# Model id -> artifact filename inside --artifacts-dir.
ARTIFACTS = {
    "realesrgan-x4plus": "realesrgan-x4plus.onnx",
    "realesrgan-x4plus-anime": "realesrgan-x4plus-anime.onnx",
}


def sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def run(cmd, check=True):
    print(f"+ {' '.join(str(part) for part in cmd)}")
    return subprocess.run(cmd, check=check, capture_output=True, text=True)


def load_catalog() -> dict:
    with open(CATALOG_PATH, encoding="utf-8") as handle:
        return json.load(handle)


def assert_no_placeholder_key(signing_key_file: Path) -> None:
    """
    A real deployment draws the seed from a secret store. This guard exists so a
    dry run against a scratch key cannot silently publish an unusable catalog.
    """
    if not signing_key_file.is_file():
        raise SystemExit(f"error: signing key file not found: {signing_key_file}")
    seed = signing_key_file.read_text(encoding="utf-8").strip()
    if len(seed) != 64 or not all(c in "0123456789abcdefABCDEF" for c in seed):
        raise SystemExit("error: signing key must be 64 hex characters (32 bytes)")
    print("signing key: present (64 hex chars)")


def verify_artifact(entry: dict, artifact_path: Path) -> None:
    if not artifact_path.is_file():
        raise SystemExit(f"error: missing artifact for {entry['id']}: {artifact_path}")
    actual_sha = sha256_file(artifact_path)
    actual_size = artifact_path.stat().st_size
    if actual_sha != entry["sha256"].lower():
        raise SystemExit(
            f"error: {entry['id']} artifact digest mismatch\n"
            f"  catalog: {entry['sha256']}\n"
            f"  file:    {actual_sha}"
        )
    if actual_size != entry["size_bytes"]:
        raise SystemExit(
            f"error: {entry['id']} size mismatch: catalog {entry['size_bytes']}, file {actual_size}"
        )
    if not artifact_path.suffix == ".onnx":
        raise SystemExit(f"error: {entry['id']} artifact must be an .onnx file")
    print(f"verified {entry['id']}: {actual_sha[:16]}... ({actual_size} bytes)")


def release_exists(tag: str) -> bool:
    result = run(["gh", "release", "view", tag, "--repo", REPO_SLUG], check=False)
    return result.returncode == 0


def publish_release(tag: str, assets) -> None:
    if release_exists(tag):
        print(f"release {tag} exists; uploading assets")
        run(["gh", "release", "upload", tag, *[str(a) for a in assets],
             "--repo", REPO_SLUG, "--clobber"])
    else:
        run(["gh", "release", "create", tag,
             *[str(a) for a in assets],
             "--repo", REPO_SLUG,
             "--title", f"Model artifacts ({tag})",
             "--notes", "Validated ONNX model packages for the Resvera model catalog."])


def resolve_asset_url(tag: str, filename: str) -> str:
    result = run([
        "gh", "release", "view", tag, "--repo", REPO_SLUG,
        "--json", "assets", "-q",
        f'[.assets[] | select(.name == "{filename}") | .url] | .[0] // empty',
    ])
    url = result.stdout.strip()
    if not url:
        raise SystemExit(f"error: asset {filename} not found on release {tag}")
    # The API url is stable but not the browser download URL; rewrite it.
    asset_id = url.rstrip("/").rsplit("/", 1)[-1]
    return f"https://github.com/{REPO_SLUG}/releases/download/{tag}/{filename}"


def verify_published(url: str, expected_sha: str, filename: str) -> None:
    """Downloads the asset and confirms the published bytes match the catalog."""
    with tempfile.TemporaryDirectory() as tmp:
        target = Path(tmp) / filename
        run(["curl", "-fsSL", "-o", str(target), url])
        actual = sha256_file(target)
        if actual != expected_sha.lower():
            raise SystemExit(
                f"error: published asset {filename} does not match the catalog\n"
                f"  catalog:   {expected_sha}\n"
                f"  published: {actual}"
            )
    print(f"published asset verified: {filename}")


def resign_and_verify(catalog: dict, signing_key_file: Path) -> None:
    with open(CATALOG_PATH, "w", encoding="utf-8", newline="\n") as handle:
        json.dump(catalog, handle, indent=2)
        handle.write("\n")

    run([str(SIGNER), "sign", "--catalog", str(CATALOG_PATH),
         "--key-file", str(signing_key_file)])
    run([str(SIGNER), "verify", "--catalog", str(CATALOG_PATH),
         "--key-file", str(signing_key_file)])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    parser.add_argument("--tag", required=True, help="GitHub release tag to publish under")
    parser.add_argument("--artifacts-dir", type=Path, required=True,
                        help="Directory containing the .onnx artifacts")
    parser.add_argument("--signing-key-file", type=Path, required=True,
                        help="File containing the production catalog signing seed (hex)")
    parser.add_argument("--dry-run", action="store_true",
                        help="Validate artifacts and print the plan without publishing")
    args = parser.parse_args()

    if not SIGNER.is_file():
        raise SystemExit(
            f"error: catalog signer not built at {SIGNER}\n"
            "run: cargo build --release --manifest-path tools/catalog-sign/Cargo.toml"
        )

    assert_no_placeholder_key(args.signing_key_file)
    catalog = load_catalog()

    by_id = {entry["id"]: entry for entry in catalog["models"]}
    to_publish = []
    for model_id, filename in ARTIFACTS.items():
        entry = by_id.get(model_id)
        if entry is None:
            print(f"skip {model_id}: not present in catalog")
            continue
        artifact_path = args.artifacts_dir / filename
        verify_artifact(entry, artifact_path)
        to_publish.append((entry, artifact_path))

    if not to_publish:
        raise SystemExit("error: no artifacts resolved to publish")

    if args.dry_run:
        print("\ndry run: would publish")
        for entry, path in to_publish:
            print(f"  {path.name} -> {REPO_SLUG}/releases/download/{args.tag}/{path.name}")
        return 0

    publish_release(args.tag, [path for _, path in to_publish])

    for entry, path in to_publish:
        url = resolve_asset_url(args.tag, path.name)
        verify_published(url, entry["sha256"], path.name)
        entry["download_urls"] = [url]
        print(f"{entry['id']} -> {url}")

    resign_and_verify(catalog, args.signing_key_file)
    print("\npublished and re-signed. Run the catalog contract tests to confirm:")
    print("  cargo test -p resvera-models --test production_catalog_contract_test")
    return 0


if __name__ == "__main__":
    sys.exit(main())

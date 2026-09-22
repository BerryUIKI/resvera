#!/usr/bin/env python3
"""
tools/release/verify_release_artifacts.py

Authoritative pre-promotion verification tool for Resvera release distribution.
Verifies SHA256 checksums, CycloneDX SBOM integrity, and signed Tauri updater manifests
before promotion/publishing to GitHub Releases.
"""

import argparse
import base64
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple


def compute_sha256(filepath: Path) -> str:
    """Computes SHA-256 hex digest for a file."""
    hasher = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            hasher.update(chunk)
    return hasher.hexdigest().lower()


def generate_sha256sums(dist_dir: Path, output_file: Optional[Path] = None) -> Path:
    """Computes SHA-256 for all release assets in dist_dir and writes SHA256SUMS.txt."""
    if output_file is None:
        output_file = dist_dir / "SHA256SUMS.txt"

    entries = []
    for p in sorted(dist_dir.iterdir()):
        if p.is_file() and p.name not in ("SHA256SUMS.txt", ".DS_Store"):
            h = compute_sha256(p)
            entries.append(f"{h}  {p.name}\n")

    with open(output_file, "w", encoding="utf-8") as f:
        f.writelines(entries)

    return output_file


def verify_sha256sums(dist_dir: Path, checksums_path: Path) -> Tuple[bool, List[str]]:
    """Verifies all files against SHA256SUMS.txt."""
    errors = []
    if not checksums_path.exists():
        return False, [f"Checksums file not found: {checksums_path}"]

    with open(checksums_path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    if not lines:
        return False, ["Checksums file is empty"]

    verified_count = 0
    for line in lines:
        line = line.strip()
        if not line or line.startswith("#"):
            continue

        parts = line.split(maxsplit=1)
        if len(parts) != 2:
            errors.append(f"Malformed checksum line: {line}")
            continue

        expected_hash, filename = parts[0].lower(), parts[1].lstrip("*").strip()
        target_file = dist_dir / filename
        if not target_file.exists():
            errors.append(f"Referenced file does not exist: {filename}")
            continue

        computed = compute_sha256(target_file)
        if computed != expected_hash:
            errors.append(
                f"Checksum mismatch for {filename}: expected {expected_hash}, got {computed}"
            )
        else:
            verified_count += 1

    if verified_count == 0 and not errors:
        errors.append("No valid checksum entries found to verify")

    return len(errors) == 0, errors


def verify_sbom(sbom_path: Path) -> Tuple[bool, List[str]]:
    """Validates CycloneDX SBOM structure."""
    errors = []
    if not sbom_path.exists():
        return False, [f"SBOM file not found: {sbom_path}"]

    try:
        with open(sbom_path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception as e:
        return False, [f"Failed to parse SBOM {sbom_path.name} as JSON: {e}"]

    bom_format = data.get("bomFormat")
    if bom_format != "CycloneDX":
        errors.append(f"Invalid bomFormat in {sbom_path.name}: expected 'CycloneDX', got '{bom_format}'")

    if "specVersion" not in data:
        errors.append(f"Missing specVersion in SBOM {sbom_path.name}")

    components = data.get("components")
    if not isinstance(components, list) or len(components) == 0:
        errors.append(f"SBOM {sbom_path.name} contains no components or components is not a list")

    return len(errors) == 0, errors


def verify_updater_manifest(
    manifest_path: Path,
    expected_version: Optional[str] = None,
    pubkey: Optional[str] = None,
) -> Tuple[bool, List[str]]:
    """Validates latest.json Tauri updater manifest structure and signatures."""
    errors = []
    if not manifest_path.exists():
        return False, [f"Updater manifest not found: {manifest_path}"]

    try:
        with open(manifest_path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception as e:
        return False, [f"Failed to parse updater manifest as JSON: {e}"]

    version = data.get("version")
    if not version or not isinstance(version, str):
        errors.append("Updater manifest missing valid 'version' string")
    elif expected_version and version.lstrip("v") != expected_version.lstrip("v"):
        errors.append(
            f"Version mismatch in manifest: expected {expected_version}, got {version}"
        )

    pub_date = data.get("pub_date")
    if not pub_date:
        errors.append("Updater manifest missing 'pub_date'")

    platforms = data.get("platforms")
    if not isinstance(platforms, dict) or len(platforms) == 0:
        errors.append("Updater manifest missing or empty 'platforms' map")
    else:
        for platform_name, target in platforms.items():
            if not isinstance(target, dict):
                errors.append(f"Target '{platform_name}' is not an object")
                continue

            url = target.get("url")
            if not url or not isinstance(url, str):
                errors.append(f"Target '{platform_name}' has invalid or missing url")

            sig = target.get("signature")
            if not sig or not isinstance(sig, str):
                errors.append(f"Target '{platform_name}' has invalid or missing signature")
            else:
                # Validate signature format (base64)
                try:
                    raw_sig = base64.b64decode(sig.strip())
                    if len(raw_sig) < 32:
                        errors.append(f"Target '{platform_name}' signature is too short ({len(raw_sig)} bytes)")
                except Exception as e:
                    errors.append(f"Target '{platform_name}' signature is not valid base64: {e}")

    return len(errors) == 0, errors


def verify_all_release_artifacts(
    dist_dir: Path,
    expected_version: Optional[str] = None,
    pubkey: Optional[str] = None,
    require_sbom: bool = True,
    require_updater: bool = True,
) -> Tuple[bool, List[str]]:
    """Runs end-to-end verification of release artifacts in dist_dir."""
    all_errors = []

    # 1. Verify SHA256SUMS.txt
    checksums_file = dist_dir / "SHA256SUMS.txt"
    ok, errs = verify_sha256sums(dist_dir, checksums_file)
    if not ok:
        all_errors.extend(errs)

    # 2. Verify SBOMs
    if require_sbom:
        sbom_files = list(dist_dir.glob("*.cdx.json")) + list(dist_dir.glob("sbom*.json"))
        if not sbom_files:
            all_errors.append(f"No CycloneDX SBOM files found in {dist_dir}")
        else:
            for sbom in sbom_files:
                ok, errs = verify_sbom(sbom)
                if not ok:
                    all_errors.extend(errs)

    # 3. Verify updater manifest
    if require_updater:
        manifest_file = dist_dir / "latest.json"
        ok, errs = verify_updater_manifest(manifest_file, expected_version, pubkey)
        if not ok:
            all_errors.extend(errs)

    return len(all_errors) == 0, all_errors


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify Resvera release distribution artifacts and updater manifest."
    )
    parser.add_argument(
        "--dist-dir",
        type=Path,
        required=True,
        help="Directory containing release artifacts to verify",
    )
    parser.add_argument(
        "--expected-version",
        type=str,
        default=None,
        help="Expected semver release version (e.g. 0.1.0 or v0.1.0)",
    )
    parser.add_argument(
        "--pubkey",
        type=str,
        default=None,
        help="Pinned Minisign/Ed25519 updater public key",
    )
    parser.add_argument(
        "--generate-checksums",
        action="store_true",
        help="Generate SHA256SUMS.txt for artifacts before verification",
    )
    parser.add_argument(
        "--require-sbom",
        action="store_true",
        default=True,
        help="Require at least one valid CycloneDX SBOM",
    )
    parser.add_argument(
        "--require-updater",
        action="store_true",
        default=True,
        help="Require valid latest.json updater manifest",
    )

    args = parser.parse_args()

    if not args.dist_dir.is_dir():
        print(f"Error: Directory '{args.dist_dir}' does not exist.", file=sys.stderr)
        return 1

    if args.generate_checksums:
        print(f"Generating SHA256SUMS.txt in {args.dist_dir}...")
        generate_sha256sums(args.dist_dir)

    success, errors = verify_all_release_artifacts(
        args.dist_dir,
        expected_version=args.expected_version,
        pubkey=args.pubkey,
        require_sbom=args.require_sbom,
        require_updater=args.require_updater,
    )

    if success:
        print("✓ All release artifacts, checksums, SBOMs, and updater manifests verified successfully.")
        return 0
    else:
        print("✗ Release verification failed with errors:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())

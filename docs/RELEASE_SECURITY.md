# Resvera Production Release Security, Signing, SBOM & Updater Specification

This document defines the cryptographic trust roots, key custody protocols, code signing, Software Bill of Materials (SBOM), build provenance attestations, updater publication, and incident rollback procedures for Resvera production releases.

---

## 1. Threat Model & Trust Boundaries

The Resvera release distribution pipeline is designed to prevent:
1. **Malicious Binary Tampering & Man-in-the-Middle (MITM):** Attackers modifying installer binaries, updates, or model weights during distribution.
2. **Supply Chain Contamination:** Compromised third-party dependencies introducing backdoors or vulnerabilities into release builds.
3. **Downgrade / Replay Attacks:** Attackers forcing users onto older, vulnerable versions through manipulated updater feeds.
4. **Key Compromise & Rogue Signing:** Unauthorized issuance of binaries or updater manifests using compromised signing identities.
5. **Unauthorized Network Exfiltration:** Release artifacts or updaters bypassing the zero-network inference boundary.

---

## 2. Cryptographic Trust Roots & Keys

Resvera establishes distinct, cryptographically isolated trust roots for each release function:

| Key Function | Algorithm / Format | Primary Custody | Public Key / Pinned Root |
|---|---|---|---|
| **Windows Authenticode** | RSA 4096 / ECC P-384 (X.509) | FIPS 140-2 Level 2+ HSM / Azure Trusted Signing | Microsoft Root CA trust chain |
| **macOS Developer ID** | Apple Developer ID (X.509) | Apple Developer Portal / Encrypted CI Keyring | Apple Root CA trust chain |
| **Model Catalog Root** | Ed25519 (RFC 8032) | Cold Storage / Air-gapped HSM | `crates/resvera-models/src/catalog.rs` (`RESVERA_PRODUCTION_TRUST_ROOT`) |
| **Tauri Updater Root** | Minisign / Ed25519 | Offline Hardware Token / GitHub Encrypted Secrets | `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) |
| **SLSA Provenance** | Sigstore / GitHub Attestations | OIDC Short-lived Workload Identity | GitHub Attestation Trust Root |

---

## 3. Key Custody and Secret Management

### 3.1 Hardware Security Module (HSM) & Cloud KMS
- **Production Private Keys:** Production private signing keys for Windows Authenticode and macOS Developer ID must never be stored on unencrypted local disks or developer workstations.
- Keys must be hosted in **FIPS 140-2 Level 2+ certified HSMs** or cloud-native signing services (e.g., Azure Trusted Signing, Google Cloud KMS, or AWS KMS).
- Access to HSM signing operations requires **multi-factor authentication (MFA)** and dual-maintainer authorization for release operations.

### 3.2 CI/CD Secret Protection
- Secrets injected into GitHub Actions workflows (`WINDOWS_CERTIFICATE_BASE64`, `APPLE_CERTIFICATE`, `TAURI_SIGNING_PRIVATE_KEY`) must be configured within a dedicated **GitHub Environment** named `production-release`.
- The `production-release` environment must require:
  1. Required reviewers (at least 2 core maintainers).
  2. Tag protection rules allowing only signed tags matching `v*.*.*`.
  3. No access permitted to pull request workflows or fork runs.

---

## 4. Platform Code Signing & Notarization

### 4.1 Windows Authenticode Signing
Windows installers (`.msi` and `.exe`) must be signed with a valid Microsoft-trusted Authenticode certificate with timestamping:
- **Timestamp Server:** `http://timestamp.digicert.com` or `http://timestamp.sectigo.com` (RFC 3161 compliant).
- **Tooling:** Windows SDK `signtool.exe` or Azure Trusted Signing CLI.
- **Verification Command:**
  ```powershell
  Get-AuthenticodeSignature -FilePath "dist/Resvera_0.1.0_x64-setup.exe"
  ```
- The signature status must report `Valid` before release promotion.

### 4.2 macOS Developer ID Signing & Apple Notarization
macOS application bundles (`.app`) and disk images (`.dmg`) must be signed and notarized:
1. **Hardened Runtime:** Compiled with `--options runtime` and entitlements preventing arbitrary dyld injection.
2. **Developer ID Application Signing:**
   ```bash
   codesign --force --options runtime --sign "Developer ID Application: Resvera LLC (TEAMID)" --timestamp "dist/Resvera.app"
   ```
3. **Apple Notary Service Submission:**
   ```bash
   xcrun notarytool submit "dist/Resvera.dmg" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
   ```
4. **Stapling:**
   ```bash
   xcrun stapler staple "dist/Resvera.dmg"
   ```

---

## 5. Software Bill of Materials (SBOM) & Provenance Attestation

Every release produces comprehensive machine-readable SBOMs and verifiable provenance:

### 5.1 CycloneDX SBOMs
- **Rust Workspace:** Generated using `cargo-cyclonedx` specifying CycloneDX 1.5 JSON:
  ```bash
  cargo cyclonedx --all --output-format json
  ```
  Generates `sbom-rust.cdx.json` covering all transitive Rust dependencies, licenses, and SHA-256 hashes.
- **Frontend / Node Dependencies:** Generated using `@cyclonedx/cyclonedx-npm`:
  ```bash
  pnpm dlx @cyclonedx/cyclonedx-npm --output-file sbom-node.cdx.json
  ```
  Generates `sbom-node.cdx.json` covering all frontend npm packages.

### 5.2 SLSA Build Provenance Attestation
- Build provenance is cryptographically attested using `actions/attest-build-provenance@v2`.
- Verifies the exact GitHub workflow run, repository commit, build inputs, and digest of all artifacts in `dist/*`.
- Third-party verifiers can inspect provenance using GitHub CLI:
  ```bash
  gh attestation verify "Resvera_0.1.0_x64-setup.exe" --owner BerryUIKI
  ```

---

## 6. Pre-Promotion Verification & Checksums

Before any release asset is published or promoted from staging to public distribution:
1. **Checksum Generation:** `SHA256SUMS.txt` is calculated covering all release binaries, installers, updater packages, and SBOMs.
2. **Automated Verification Gate:** `tools/release/verify_release_artifacts.py` executes:
   - Validates all files against `SHA256SUMS.txt`.
   - Validates `latest.json` updater manifest structure, platform entries, URLs, and signatures.
   - Validates CycloneDX SBOMs for format and component validity.
   - Fails closed with non-zero exit code if any artifact is missing, corrupt, tampered, or improperly signed.

---

## 7. Tauri Updater Publication & Client Protection

The Tauri updater operates on a signed `latest.json` manifest:
- **Manifest URL:** `https://github.com/BerryUIKI/resvera/releases/latest/download/latest.json`
- **Signing Tool:** `tauri signer sign` using Minisign private key.
- **Public Key Pinning:** Pinned in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
- **Client Downgrade Protection:**
  - The updater client compares semantic versions monotonically.
  - Releases with lower or equal versions are rejected.
  - Updates with invalid or unpinned signatures abort without executing installers.

---

## 8. Key Rotation Protocol

### 8.1 Routine Rotation Schedule
- **Code Signing Certificates (X.509):** Rotated every 12–24 months upon certificate renewal.
- **Minisign Updater Keypair:** Rotated every 24 months.
- **Model Catalog Signing Key:** Rotated every 24 months.

### 8.2 Dual-Key Transition Window (Updater)
To prevent stranding older client installations when rotating the updater public key:
1. Version `N` is published with the existing public key `Key_A` and an updated binary supporting `Key_B`.
2. A dual-signature manifest or transition release is published where older clients verify with `Key_A` and upgrade to version `N`.
3. Subsequent releases transition exclusively to `Key_B`.

---

## 9. Key Revocation & Compromise Response Plan

If any signing key or certificate is suspected of compromise:

### 9.1 Immediate Response Timeline (< 2 Hours)
1. **Revoke Certificate at CA:** Notify the issuing Certificate Authority (DigiCert, Sectigo, Apple) immediately to publish CRL / OCSP revocation.
2. **Deactivate CI/CD Secrets:** Remove all signing secrets from GitHub repository environment immediately.
3. **Deploy Updater Killswitch:** Publish an emergency `latest.json` manifest with empty platforms or a retracted version manifest to freeze automatic update downloads across all active clients.

### 9.2 Model Catalog Revocation
If the model catalog private key is compromised:
1. Remove all untrusted catalog packages from download mirrors.
2. Deploy an application hotfix containing an updated pinned `RESVERA_PRODUCTION_TRUST_ROOT`.
3. The application will immediately fail closed on catalogs signed with the compromised key.

---

## 10. Emergency Release Rollback Protocol

In the event a critical flaw, regression, or malicious injection is detected in a distributed release:
1. **GitHub Release Yanking:**
   - Immediately mark the GitHub Release as `Draft` or delete the release tag.
   - The release assets become unavailable for download.
2. **Updater Manifest Neutralization:**
   - Update `latest.json` on the update server/release mirror to point to a known-stable release or an empty manifest:
     ```json
     {
       "version": "0.1.0",
       "notes": "Emergency rollback: automatic updates temporarily paused for maintenance.",
       "pub_date": "2026-09-22T00:00:00Z",
       "platforms": {}
     }
     ```
3. **Hotfix Patch Release:**
   - Prepare a hotfix release with an incremented patch version (e.g. `v0.1.1`).
   - Run complete CI and release verification workflows.
   - Promote hotfix with detailed advisory and mitigation instructions.

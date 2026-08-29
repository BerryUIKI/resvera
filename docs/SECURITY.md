# Resvera Security Architecture & Threat Model

## 1. Core Principles & Offline Isolation

Resvera is built strictly on the principle of **Zero-Network Inference Isolation**:
- **Offline By Default**: Model loading, preprocessing, inference execution (CPU, DirectML, CoreML, CUDA), seamless tile feathering, postprocessing, metadata filtering, and disk writing operate 100% locally with zero outbound network calls.
- **Explicit Acquisition Gate**: Network connections are exclusively permitted during user-initiated model catalog downloads or runtime package updates.

## 2. Threat Model & Mitigation Matrix

| Threat / Attack Vector | Risk | Defense & Enforcement Mechanism |
|---|---|---|
| **Path Traversal Attacks** (`../`, `C:\...`) | High | Strict relative path validation in `ModelManifest::validate()` and filename sanitization in `sanitize_filename_component()`. |
| **Tampered Model Weights / MITM** | Critical | End-to-end Ed25519 cryptographic signatures on catalogs and per-chunk SHA-256 validation on staged model downloads. |
| **Windows DOS Device Names** (`CON`, `PRN`, `NUL`) | Medium | Automated prefixing with `_file` suffix during filename formatting in `resvera-core`. |
| **Sensitive Path / PII Leaks in Logs** | Medium | Automated path anonymization in `DiagnosticCollector` replacing all user home directories with `<USER_DIR>`. |
| **WebView Asset Scope Escape** | High | Tauri v2 ACL capabilities limiting protocol access strictly to cache-scoped preview paths. |

## 3. Cryptographic Signature Verification

All official model packages and runtime components distributed through the catalog are signed using **Ed25519 (Edwards-curve Digital Signature Algorithm)**.

The verification process follows:
1. `SignedCatalog` is fetched over TLS with signature header.
2. The payload is verified against the pinned public key (`ed25519-dalek`).
3. Each individual binary artifact is validated against its recorded SHA-256 hash.
4. If validation fails or the connection is interrupted, the installation directory is rolled back cleanly.

## 4. Reporting Vulnerabilities

If you discover a potential security vulnerability in Resvera, please file a confidential security report via GitHub Security Advisories or contact the maintainers directly.

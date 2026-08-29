# Resvera Technology Stack

## 1. Technology Decision

Resvera uses Tauri v2, Rust, SolidJS, and ONNX Runtime. ONNX Runtime is the sole inference engine in the initial release. Platform acceleration is supplied by Execution Providers, while the default CPU provider is always available.

This decision prioritizes offline operation, model portability, explicit fallback behavior, and a stable path to future inference engines. Bundle size is not a primary product constraint.

## 2. Desktop and Frontend

| Area | Technology | Role |
|---|---|---|
| Desktop shell | Tauri v2 | Native windowing, IPC, updater integration, and security boundaries |
| Backend | Rust 2021 or newer pinned edition | Queue, image pipeline, model management, runtime integration, and local persistence |
| Async runtime | Tokio | Bounded asynchronous I/O and background work |
| Frontend | SolidJS + TypeScript | Reactive desktop interface and typed IPC client |
| Build tool | Vite | Frontend development and production bundling |
| Styling | Tailwind CSS | Design tokens and application styling |
| Icons | Lucide Solid | UI icons |

The frontend does not receive broad filesystem or shell permissions. Privileged work remains in Rust commands with explicit validation.

## 3. Inference Runtime

### 3.1 Engine

The production runtime is ONNX Runtime through its stable C API. Rust integration may use the `ort` crate internally, but no `ort` type is allowed outside the runtime adapter. This contains binding changes and preserves the application-owned `InferenceEngine` contract.

ONNX Runtime, the `ort` binding, and every provider binary are pinned to an exact compatible version set. The application does not compile against “latest stable” ranges.

### 3.2 Execution Providers

Provider support is packaged per platform rather than in one universal runtime bundle.

| Platform | Required providers | Optional providers |
|---|---|---|
| Windows | CPU, DirectML compatibility path | Windows ML-discovered providers |
| macOS ARM64/x64 | CPU, CoreML | None initially |
| Linux x64 | CPU | CUDA and OpenVINO distributions |

Linux AMD GPU acceleration is not promised in the initial non-Vulkan architecture. Those systems remain supported through the CPU provider.

Provider packages are treated as signed runtime components. They may be installed with the application or downloaded through an explicit component-update workflow. Inference never downloads them on demand.

### 3.3 Session Policy

- One active inference session at a time in the initial release.
- Session cache keys include engine version, provider configuration, model artifact hash, precision, and static tile shape.
- Provider-native compiled caches are stored locally and may be rebuilt safely.
- Compiled provider artifacts are never distributed between machines.
- CPU execution supports FP32; accelerated providers may use validated FP16 paths.
- A model package declares which provider/precision combinations passed parity testing.

## 4. Rust Components

The following dependency categories are expected. Exact versions must be recorded in `Cargo.toml` and `Cargo.lock` when the project is scaffolded.

| Component | Purpose |
|---|---|
| `tauri` and official plugins | Desktop runtime, dialogs, updater, and process lifecycle |
| `ort` behind an internal adapter | ONNX Runtime binding |
| `tokio` | Async tasks, file I/O, cancellation coordination |
| `serde`, `serde_json` | Versioned IPC and manifest serialization |
| `image` | Initial PNG/JPEG/WebP decode and encode pipeline |
| `fast_image_resize` | Lanczos3 final resizing |
| `kamadak-exif` and container-specific metadata support | Selective metadata preservation |
| `sha2` | SHA-256 artifact verification |
| Ed25519 verification library | Signed catalog verification |
| `uuid` | Job and installation transaction identifiers |
| `thiserror` | Typed internal errors |
| `tracing`, `tracing-subscriber` | Structured local diagnostics |
| Embedded database or transactional store selected by ADR | Persistent queue and component registry |

The persistence implementation must be selected in an Architecture Decision Record before queue implementation. A transactional embedded database is preferred over rewriting a single JSON file because jobs, history, downloads, and component versions require crash-consistent updates.

## 5. Image Processing

Image file handling is independent of model inference:

1. Decode the source container in Rust.
2. Normalize orientation and channel layout.
3. Convert tiles to the model's declared tensor format.
4. Run local inference.
5. Blend overlap and reconstruct the result.
6. Perform final scale conversion when needed.
7. Apply the metadata policy.
8. Encode to a temporary output.
9. Atomically commit the final file.

The initial supported formats are PNG, JPEG, and WebP. Alpha, grayscale, and higher bit depth must have explicit test vectors before they are advertised.

## 6. Model Toolchain

Production model packages are not arbitrary third-party ONNX downloads. Resvera maintains a reproducible export and validation toolchain outside the desktop application:

- pin the upstream source revision and original weight hash;
- export with a pinned Python, PyTorch, ONNX, and opset version;
- run ONNX validation and graph optimization;
- compare output against the official reference implementation;
- test every advertised provider and precision;
- generate a signed manifest and SHA-256 file list;
- publish the immutable package to the model catalog.

Export scripts and validation fixtures are version-controlled. Model conversion is a release-engineering activity, not an end-user operation.

## 7. Networked Components

Network capability is limited to three explicit services:

- model catalog and model package download;
- runtime component catalog and provider package download;
- application update check and download.

All three require HTTPS, signed metadata, hash verification, resumable temporary downloads, and atomic installation. Automatic checks are configurable. No networking library is linked into the core inference crate.

## 8. Build and Distribution

CI produces separate artifacts for:

- Windows x64;
- macOS ARM64;
- macOS x64;
- Linux x64 CPU;
- optional Linux x64 CUDA/OpenVINO component packages.

Each build records:

- Rust toolchain version;
- Node.js and package-manager version;
- ONNX Runtime and provider versions;
- model catalog public-key version;
- dependency lockfile hashes;
- generated Software Bill of Materials;
- artifact checksums and signatures.

Code signing and notarization are release requirements, not optional post-release work.

## 9. Versioning Policy

- Rust and frontend dependencies are pinned by lockfiles committed to source control.
- Toolchain versions are pinned in repository configuration.
- ONNX Runtime and provider packages use an explicit compatibility matrix.
- Model packages are immutable; an update creates a new package version and hash.
- Catalog schemas and IPC contracts are versioned.
- Runtime and model rollback remain possible after a failed update.
- Dependency updates are never applied while an inference job is running.

## 10. Deferred Alternatives

MNN and ncnn are possible future engines, but neither is implemented in the initial release. Burn/WGPU is not selected because its ONNX operator coverage and deployment stability must mature before it can satisfy the supported model set.

A future engine is admitted only when it:

- implements the stable `InferenceEngine` interface;
- passes the same golden-image and numerical parity suite;
- uses signed model artifacts;
- supports cancellation and bounded memory behavior;
- does not introduce a cloud dependency;
- does not require changes to queue or IPC contracts.

# ADR-0004: Strict Offline Inference and Isolated Network Boundary

## Status
Accepted

## Context
Resvera is fundamentally designed around user privacy and offline sovereignty. Desktop image upscaling and restoration must function with zero network access once required models and runtimes are installed. Users must have complete certainty that:
1. No source images, output images, previews, filenames, EXIF metadata, or tensor activations are ever transmitted over a network.
2. Inference jobs never fail due to lack of an internet connection.
3. No implicit background downloads, cloud fallbacks, or silent telemetry occur during image processing.

## Decision
1. **Architectural Decoupling:**
   - The core image processing and inference crates (`resvera-core`, `resvera-engine-ort`, `resvera-models`) must contain **no HTTP client, networking library, or socket communication code**.
   - Network operations are confined to isolated download modules (`resvera-updater`, `resvera-catalog-client`) that are invoked strictly on explicit user actions (e.g. clicking "Download Model" or "Check for Updates").
2. **Offline Inference Guarantee:**
   - Image decoding, tiling, preprocessing, tensor evaluation, post-processing, and output encoding execute exclusively in local CPU / GPU memory.
   - If an installed model or runtime component is missing or corrupt, Resvera surfaces a clear, actionable error (`modelNotInstalled` / `engineUnavailable`) rather than attempting an implicit on-demand download.
3. **Telemetry & Privacy Default:**
   - Telemetry is disabled by default.
   - ONNX Runtime native telemetry provider hooks (e.g. Windows ETW telemetry flags) are explicitly deactivated during ORT environment initialization.
4. **Local Preview Isolation:**
   - Previews are cached locally in the application cache and served to the WebView frontend through a restricted Tauri custom asset protocol (`asset://localhost/previews/`). Full-resolution image paths and system root filesystems are forbidden from WebView URL access.

## Consequences
### Positive
- Air-gapped and offline environments are first-class supported targets.
- Verifiable privacy model for high-security, sensitive, and air-gapped image workflows.
- Clean dependency graph preventing network libraries from contaminating inference code.

### Negative / Trade-offs
- Users must explicitly initiate downloads for new models or runtime updates rather than enjoying "automatic zero-click auto-fetch".

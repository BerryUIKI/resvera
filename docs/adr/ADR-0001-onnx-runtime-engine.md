# ADR-0001: ONNX Runtime as the Unified Inference Engine and Execution Provider Architecture

## Status
Accepted

## Context
Resvera requires a robust, local-first image super-resolution and restoration engine. The core constraints are:
1. Complete offline capability: once installed, inference must never access the network or depend on external server processes.
2. Cross-platform hardware acceleration: support for Windows (DirectML / Windows ML), macOS (CoreML), Linux (CUDA / OpenVINO), and universal CPU fallback on all platforms.
3. Decoupled architectural boundaries: the engine abstraction (`InferenceEngine`) must remain stable and separate from the queue, image pipeline, and model adapter (`ModelAdapter`), allowing future alternative engines (e.g. MNN, ncnn) without altering the IPC or UI contracts.
4. Process safety and resource management: avoid fragile subprocess sidecars (e.g. ncnn CLI sidecars) that can become zombie processes or suffer from IPC serialization overhead.

## Decision
1. **Adopt ONNX Runtime (ORT) C-API as the sole initial inference engine.**
   - In-process execution through the stable ONNX Runtime C API (interfaced via Rust's `ort` crate internally).
   - Platform acceleration is realized strictly through ONNX Runtime **Execution Providers (EPs)**: CPU (built-in fallback), DirectML, CoreML, CUDA, and OpenVINO.
2. **Encapsulate ORT behind an application-owned `InferenceEngine` trait.**
   - No `ort` binding types, ONNX session handles, or C pointers may cross the `InferenceEngine` boundary.
   - Standardized tensor views (`TensorView<'a>`) and owned tensors (`OwnedTensor`) represent inputs and outputs.
   - Capability probing (`Engine::probe()`) queries available and healthy providers dynamically at runtime.
3. **Session Lifecycle & Concurrency Policy:**
   - Enforce a strict single-active-inference-session rule for the initial release to bound VRAM/RAM consumption and prevent GPU driver contention.
   - Support dynamic tile sizing and static shape compilation caches where necessary for specific hardware/EP optimizations.
4. **Error Handling & OOM Recovery:**
   - Out-Of-Memory (OOM) and device-lost errors returned by ORT are caught, mapped to typed `EngineError::OutOfMemory`, and handled cooperatively in the tile pipeline by reducing tile dimensions or falling back to CPU according to job policy.

## Consequences
### Positive
- Unified graph optimization, operator support, and tensor memory management across all hardware backends.
- Zero sidecar processes; in-process cancellation and memory control.
- Clear separation between model math (Model Adapter), runtime execution (ORT Engine), and task orchestration (Job Queue).

### Negative / Trade-offs
- ONNX Runtime shared libraries and EP dynamic libraries increase distribution bundle size (accepted as per `TECH_STACK.md`).
- Dynamic shapes in some model architectures (e.g. Vision Transformers / HAT) may require static tile dimension quantization on certain providers.

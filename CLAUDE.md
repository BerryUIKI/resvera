# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Resvera is an offline desktop image upscaler/restorer: Tauri v2 shell, Rust backend, SolidJS frontend, ONNX Runtime inference in-process. Image decoding, tiling, inference, metadata filtering, and encoding never touch the network. Default branch is `dev`; PRs target `dev`.

## Toolchain (pinned — do not float)

| Tool | Version | Source of truth |
|---|---|---|
| Rust | 1.97.1 | `rust-toolchain.toml` |
| Node | 22.13.0 | `.node-version`, `package.json#engines` |
| pnpm | 11.18.0 | `package.json#packageManager` |
| Python | 3.12.7 | `.python-version` |
| `ort` crate | `=2.0.0-rc.13` | workspace `Cargo.toml` |

Python export/parity tooling uses exact `==` pins in `tools/export/requirements.txt` (torch 2.4.1, onnx 1.16.2, onnxruntime 1.19.2). See `docs/TOOLCHAIN.md` for upgrade procedures — version bumps go through that document's steps, not ad-hoc edits.

## Commands

```bash
pnpm install                    # frontend deps
pnpm run dev                    # Vite only (port 1420) — browser preview mode, no inference
pnpm tauri dev                  # full desktop app
pnpm run check                  # tsc --noEmit
pnpm run test                   # vitest run
pnpm exec vitest run src/test/queue.test.ts          # single frontend test
pnpm run build                  # vite build

cargo test --workspace
cargo test -p resvera-core --test orchestrator_test  # single Rust integration test binary
cargo test -p resvera-core --test orchestrator_test <substring>  # single test fn
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --manifest-path src-tauri/Cargo.toml --no-default-features  # CI no-bundle check

bash scripts/check-hygiene.sh          # fmt + clippy + python tests + git diff --check (scripts/check-hygiene.ps1 on PowerShell)
python -m unittest discover -s tools/export
python -m unittest discover -s tools/release
pnpm tauri build                # release bundles; needs TAURI_SIGNING_PRIVATE_KEY
```

CI (`.github/workflows/ci.yml`) gates on: `git diff --check` against the base ref, `cargo fmt --check`, `clippy -D warnings`, Python unittest discovery in `tools/export` and `tools/release`, `pnpm check && pnpm test && pnpm build`, `cargo test --workspace`, and the no-bundle Tauri build.

## Workspace layout

```
crates/resvera-core        # pipeline, adapters, orchestrator, engine trait
crates/resvera-engine-ort  # OrtEngine — only crate allowed to touch `ort`
crates/resvera-models      # catalog, manifest, signing, downloader, installer, runtime
crates/resvera-persistence # SQLite job queue, migrations, keyset pagination
src-tauri                  # Tauri commands, IPC types, queue worker, preview scope
src                        # SolidJS UI, i18n, typed API client
tools/                     # Python export / parity / release verification
```

Dependency direction is strictly `core → models/persistence`, `engine-ort → core`, `src-tauri → all`. `resvera-core`, `resvera-engine-ort`, and `resvera-models` must contain **no HTTP client or socket code** — networking lives only in `src-tauri/src/commands.rs` (model download via `ureq`) triggered by explicit user actions. `docs/adr/ADR-0004-offline-network-boundary.md` is the rule; violating it is a release blocker.

## Architecture you have to read several files to see

**Job flow.** Tauri commands (`src-tauri/src/commands.rs`) validate input → `JobOrchestrator::submit_job` (`crates/resvera-core/src/orchestrator.rs`) writes a `JobRecord` to SQLite and returns only after commit. A dedicated OS thread, `QueueWorker` (`src-tauri/src/worker.rs`), polls `orchestrator.process_next_job()` and is the only thing that runs inference. The UI never pulls work.

**Single job execution** (`orchestrator.rs::execute_job`): decode + orientation normalize → resolve installed model via `ModelInstaller::resolve_version_variant` (reads `current.json` pointer) → pick `ModelAdapter` by manifest `family` → build `TilePlan` → per-tile preprocess/`engine.run`/postprocess/blend → native-scale or Lanczos3 downsample → `atomic_save_image_with_metadata` → write preview → persist success. Terminal states are only recorded after the file is committed; on cancel/failure, partially written outputs are removed and the job's staged input is cleaned up.

**Three abstractions must stay separate** (`docs/ARCHITECTURE.md` §3): `InferenceEngine` (no `ort` types may cross it), execution providers (not separate engines — DirectML/CoreML/CUDA are all `OrtEngine`), and `ModelAdapter` (model-family semantics: tensor layout, tiling constraints, pre/postprocess). To add a model family, implement `ModelAdapter` and extend `adapter_for_manifest`; do not add family-specific arguments to `OrtEngine`.

**Provider reality today:** `OrtEngine::capabilities()` reports `vec!["cpu"]` and `resolve_provider` rejects anything else with `EngineError::Provider` (`crates/resvera-engine-ort/src/engine.rs`). Accelerators are intentionally fail-closed pending hardware parity evidence (#68). Do not advertise or enable them in code or docs.

**Persistence** (`crates/resvera-persistence/src/db.rs`): SQLite WAL, single-writer. `run_crash_recovery_sweep()` flips `preparing`/`running`/`finalizing` to `interrupted` at startup and is called from `main.rs`. Job history uses opaque keyset cursors (`encode_cursor`/`decode_cursor`), not OFFSET.

**IPC contract:** Rust types are canonical. Wire rules — camelCase fields, `kind`-tagged enums with camelCase values, RFC 3339 UTC timestamps, big byte counts as decimal strings, structured `ApiError` (never `"CODE: message"`). Mirror changes in `src-tauri/src/ipc_types.rs` into `src/types/ipc.ts` and keep `src-tauri/tests/ipc_contract_test.rs` green.

**Previews/security:** full outputs stay wherever the user chose them; UI thumbnails go to the app cache and are served only through the Tauri asset protocol with a dynamic whitelist (`src-tauri/src/preview_scope.rs` — `allow_file` on job completion, `allow_directory` for cache/staging). `src-tauri/tauri.conf.json` has an empty static `assetProtocol.scope` on purpose; new directories must go through `PreviewScope`, not the config scope.

**Paths:** Windows verbatim prefixes (`\\?\`) leak into the WebView if not stripped — use `strip_verbatim_prefix` (Rust) / `normalizePath` (`src/lib/api.ts`) whenever a path crosses IPC or a URL.

## Frontend conventions

- SolidJS signals only — no React idioms, no store libraries. `App.tsx` holds the top-level signals; components take props.
- All privileged work goes through `src/lib/api.ts`, which branches on `isTauri()` and returns realistic stubs in browser mode. Browser mode does not infer — keep stub behavior truthful rather than faking success.
- i18n is reactive via `useI18n().t("a.b.c")` with nested keys in `src/i18n/locales/en.ts` and `zh.ts`. Both locales must be updated together; the contract is single-language per locale (no bilingual strings).

## Testing

- Rust integration tests live in `crates/*/tests/` and `src-tauri/tests/`. Orchestrator/IPC tests use `MockEngine` + a deterministic model fixture from `crates/resvera-core/tests/common/mod.rs` — passing against mocks is not evidence for a release claim (see `docs/REMEDIATION_ROADMAP.md` §2).
- Model artifacts (`artifacts/`, `*.onnx`, `*.pth`) are gitignored. Export-dependent code looks in `artifacts/exports/<model-id>/model.onnx` (walking up two levels) or the path in `RESVERA_TEST_MODEL_FIXTURE`.
- Frontend tests are in `src/test/`, jsdom + `@testing-library/jest-dom`, setup at `src/test/setup.ts`.

## Claims discipline

This repo treats unsupported capability claims as defects. `docs/ROADMAP.md` §10 is the pre-release checklist: nothing may be described as supported in README, UI, user guide, or comments without committed evidence (hardware parity reports, signed model packages, automated metadata tests). Real-CUGAN and Real-HAT-GAN have core adapters but no validated package, so they are excluded from the production catalog entirely and must be described as library-only. `CascadePipeline` (8x) is likewise library-only — job validation rejects `target_scale > native_scale` and the UI offers 1x/2x/4x. Test counts in docs must match `cargo test --workspace`, not estimates.

Authoritative docs to consult before changing behavior: `docs/ARCHITECTURE.md`, `docs/API_AND_IPC_SPEC.md`, `docs/MODELS_SPEC.md`, `docs/SECURITY.md`, `docs/RELEASE_SECURITY.md`, and `docs/adr/`.

## Release

Tag `v*` triggers `.github/workflows/release.yml`: three-platform Tauri bundles, Authenticode/macOS notarization, CycloneDX SBOMs, SLSA attestation, then `tools/release/verify_release_artifacts.py --dist-dir <dir>` gate. Updater trust root is the Minisign pubkey pinned in `src-tauri/tauri.conf.json`; model catalog trust root is `RESVERA_PRODUCTION_TRUST_ROOT` in `crates/resvera-models/src/catalog.rs`. Never regenerate or replace either key as part of an unrelated change.

The catalog signing seed is **not** in the repository and must never be: it is read from `RESVERA_CATALOG_SIGNING_KEY` or `--key-file` by `tools/catalog-sign/` (see its README for the rotation and signing procedure). The previous seed was committed in plaintext in the test suite and was rotated on 2026-11-01; the current `RESVERA_PRODUCTION_TRUST_ROOT` has no public seed. Any catalog edit invalidates every signature — re-sign with the tool, then confirm `cargo test -p resvera-models --test production_catalog_contract_test` still passes.

`crates/resvera-models/tests/production_catalog_contract_test.rs` is the offline gate for catalog data: it reads the committed `production.json` and checks entry/manifest agreement, template digest, provenance, HTTPS ONNX install URLs, provider claims, and that only validated packages ship. Do not "fix" a failure there by making the test read the same constants it validates — that is the defect it exists to catch.

A catalog entry with an empty `download_urls` is legal and means "no published artifact". `ModelSummary.installable` mirrors that, and the Model Center hides the Download button rather than offering a download guaranteed to fail.

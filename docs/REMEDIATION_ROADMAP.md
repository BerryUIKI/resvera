# Resvera Production Remediation Roadmap

## 1. Objective

This roadmap converts the current UI and architecture prototype into an honest,
fail-closed, locally executed image-restoration application. A feature is complete
only when its production code path, persistence behavior, security boundary, and
end-to-end tests agree. Passing tests against mocks or generated random weights is
not sufficient evidence for a release claim.

The remediation branch is cut from `dev` and returns through a pull request to
`dev`. Existing working-tree changes are preserved and reviewed as part of the
same remediation effort.

## 2. Non-negotiable release gates

1. A job may report `succeeded` only after a real output file is atomically
   committed and the success record is persisted.
2. Missing, unreadable, invalid, or incompatible model artifacts fail with a
   structured error. Placeholder model bytes and client-side "success" fallbacks
   are forbidden in the desktop application.
3. The ONNX Runtime engine must execute the supplied ONNX graph. Provider health
   and availability are probed rather than hard-coded.
4. Model catalog signatures, package identities, active versions, variants, and
   artifact hashes form one verified chain from catalog to inference session.
5. Queue ownership lives in the Rust backend. Claiming, cancellation, retry, crash
   recovery, progress, and terminal state persistence are deterministic.
6. Dimensions, scales, tile settings, providers, formats, paths, and memory budgets
   are validated at the Rust IPC boundary with checked arithmetic.
7. Documentation and UI expose only behavior supported by production code and
   repeatable evidence.

## 3. Delivery phases

### Phase R0 — Truthful baseline and source safety

- Preserve the existing dirty working tree and develop on a dedicated
  `codex/` branch cut from `dev`.
- Remove mock-success terminology and mark unavailable features as unavailable.
- Make the export tool require explicit upstream weights, an expected source hash,
  and an explicit output directory; random production weights are prohibited.
- Replace milestone checkboxes that lack evidence with accurate status markers.

Acceptance:

- No production path silently substitutes placeholder model bytes or generated
  output after a backend failure.
- Running the export command without verified weights fails before writing an
  artifact.

### Phase R1 — Real inference and model registry

- Integrate the ONNX Runtime Rust binding behind `InferenceEngine`.
- Load sessions from the selected installed manifest artifact and validate tensor
  names, layout, element type, native scale, provider, and variant.
- Resolve `current.json` transactionally and support the package layout declared in
  `MODELS_SPEC.md`.
- Keep the previously active version intact until a replacement has been fully
  verified and activated.
- Bind downloads to a verified signed catalog entry and reject identity, version,
  size, signature, or hash mismatches.

Acceptance:

- A small deterministic ONNX fixture proves that engine output depends on graph
  execution rather than interpolation code.
- Missing and corrupt models return `modelNotInstalled` or `modelInvalid` and do
  not create an output.
- Failed replacement leaves the previous active model readable and selected.

### Phase R2 — Authoritative persistent queue

- Add an atomic SQLite claim transition from `queued` to `preparing`.
- Persist normalized error code/message, attempt timestamps, progress, cancellation,
  output, and preview paths.
- Run inference in a backend worker so the frontend never chooses which database
  row `process_next_job` means.
- Hydrate frontend state from backend snapshots and apply updates strictly by job
  ID. Retry creates an explicit backend attempt; pause controls the backend queue.
- Check cancellation through merge, resize, encode, and pre-commit boundaries.

Acceptance:

- Concurrent claim tests prove a job is executed at most once.
- Restart, cancel-during-finalize, retry, and 100-job tests retain consistent
  terminal state and files.

### Phase R3 — Bounded and correct image pipeline

- Validate supported input formats and reject directories and oversized dimensions.
- Use checked arithmetic for tensor and output dimensions.
- Derive scale, artifact, tiling constraints, overlap, and padding from the selected
  manifest variant and adapter.
- Implement exact below-native scaling and explicit above-native cascade plans.
- Bound compositor memory through strips/chunks or a preflight budget; allocation
  failures become structured `outOfMemory` errors.
- Wire output format, quality, overwrite, naming, metadata, GPS, precision, provider,
  tile, overlap, blend, and model-specific controls end to end.
- Preserve or deliberately strip orientation, alpha, and metadata according to a
  tested policy; do not advertise unsupported formats or bit depths.

Acceptance:

- Exact 1x/2x/4x/8x dimensions and variant selection are covered by integration
  tests.
- A declared memory limit rejects unsafe work before allocation.
- PNG alpha and JPEG orientation/metadata policies have fixture tests.

### Phase R4 — Desktop security and cross-platform durability

- Configure a restrictive Content Security Policy.
- Enable the Tauri asset protocol only for the preview cache and reject unrelated
  paths.
- Store state under Tauri's platform application-data directory rather than a
  manually constructed home-directory path.
- Make atomic replacement work on Windows, macOS, and Linux without deleting a
  known-good destination first.
- Validate settings schema and surface write/migration failures.
- Redact sensitive paths in persisted and exported diagnostics.

Acceptance:

- Asset-scope escape, malicious settings, traversal, symlink, and overwrite tests
  pass on the CI platform matrix.
- A settings write failure is visible to the caller and does not mutate the
  in-memory committed state.

### Phase R5 — Evidence and release discipline

- Generate or mechanically verify TypeScript IPC types from canonical Rust types.
- Add frontend queue/state tests and real backend integration tests.
- Add formatting, Clippy, diff hygiene, dependency review, parity, and desktop build
  gates to CI.
- Pin the Rust toolchain and Python export environment; retain one JavaScript lock
  workflow.
- Add the declared AGPL license file and reconcile README, architecture, security,
  user guide, and roadmap claims with actual evidence.

Acceptance:

- `cargo fmt --check`, Clippy with warnings denied, the Rust workspace test suite,
  frontend tests/typecheck/build, and a Tauri no-bundle build all pass.
- No completed roadmap item lacks a linked automated test or recorded external
  verification artifact.

## 4. External evidence that cannot be fabricated in code

The repository can enforce fail-closed behavior without these inputs, but the
following release claims remain blocked until maintainers supply real evidence:

- official upstream model weights and immutable source hashes;
- production catalog public keys and protected signing workflow;
- DirectML, CoreML, CUDA, and OpenVINO hardware validation;
- platform code-signing identities, notarization credentials, and release signing;
- licensing/provenance approval for every redistributed model.

Until those inputs exist, the application and documentation must report the
corresponding model, provider, or release capability as unavailable or unverified.

## 5. Pull-request completion criteria

The remediation pull request may be self-approved and merged into `dev` only when:

1. all automated local and CI checks pass;
2. no P0 or P1 finding remains hidden behind a fallback or documentation claim;
3. external-evidence gaps are fail-closed and explicitly documented;
4. the PR contains no unrelated destructive change and preserves prior user work;
5. the branch is mergeable with the current `dev` head.

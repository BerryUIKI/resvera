# Resvera API and IPC Specification

## 1. Contract Rules

The SolidJS frontend communicates with the Rust core through Tauri v2 commands and events. Rust types are the canonical contract. TypeScript definitions are generated from those types and checked in CI; handwritten duplicate type definitions are not permitted.

Wire-format rules:

- object fields use `camelCase`;
- enum discriminators use a `kind` field with `camelCase` values;
- identifiers are opaque strings;
- filesystem paths are UTF-8 strings at the IPC boundary and validated by Rust;
- timestamps are RFC 3339 UTC strings;
- byte counts are serialized as decimal strings when they may exceed JavaScript's safe integer range;
- command errors use a structured `ApiError`, never an encoded `"CODE: message"` string;
- commands that create persistent state return only after that state is committed.

Example Rust enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutputFormat {
    SameAsInput,
    Png,
    Jpeg { quality: u8 },
    Webp { lossless: bool, quality: Option<u8> },
}
```

Matching TypeScript:

```typescript
export type OutputFormat =
  | { kind: "sameAsInput" }
  | { kind: "png" }
  | { kind: "jpeg"; quality: number }
  | { kind: "webp"; lossless: boolean; quality: number | null };
```

## 2. Shared Types

### 2.1 Errors

```typescript
export interface ApiError {
  code: ErrorCode;
  message: string;
  details: Record<string, unknown> | null;
  retryable: boolean;
}

export type ErrorCode =
  | "invalidArgument"
  | "fileNotFound"
  | "unsupportedFormat"
  | "outputConflict"
  | "modelNotFound"
  | "modelNotInstalled"
  | "modelInvalid"
  | "modelInUse"
  | "engineUnavailable"
  | "providerUnavailable"
  | "providerIncompatible"
  | "outOfMemory"
  | "cancelled"
  | "jobNotFound"
  | "downloadFailed"
  | "signatureInvalid"
  | "hashMismatch"
  | "updateUnavailable"
  | "permissionDenied"
  | "storageFailure"
  | "internal";
```

User-facing error text is localized in the frontend using `code` and safe structured details. Backend `message` is a diagnostic fallback and must not contain secrets.

### 2.2 Engine and Provider Status

```typescript
export interface RuntimeStatus {
  engine: EngineInfo;
  providers: ProviderInfo[];
  automaticProviderOrder: string[];
  offlineReady: boolean;
}

export interface EngineInfo {
  id: string;
  displayName: string;
  version: string;
  healthy: boolean;
  diagnostic: string | null;
}

export interface ProviderInfo {
  id: string;
  displayName: string;
  version: string | null;
  installed: boolean;
  available: boolean;
  deviceName: string | null;
  dedicatedMemoryBytes: string | null;
  diagnostic: string | null;
}
```

`offlineReady` means that the engine, CPU provider, and at least one installed model package are valid. It does not indicate network connectivity.

```typescript
export interface RuntimeComponentSummary {
  id: string;
  displayName: string;
  installedVersion: string | null;
  catalogVersion: string | null;
  installed: boolean;
  updateAvailable: boolean;
  active: boolean;
  downloadSizeBytes: string | null;
  compatible: boolean;
  diagnostic: string | null;
}
```

### 2.3 Models

```typescript
export interface ModelSummary {
  id: string;
  packageVersion: string;
  displayName: string;
  family: string;
  category: "photo" | "anime" | "document";
  nativeScales: number[];
  installed: boolean;
  updateAvailable: boolean;
  downloadSizeBytes: string | null;
  licenseSpdx: string;
  redistributionReview: "approved" | "pending" | "rejected";
  validatedProviders: string[];
  variants: ModelVariantSummary[];
}

export interface ModelVariantSummary {
  id: string;
  nativeScale: number;
  strength: string | null;
}
```

### 2.4 Jobs

```typescript
export type JobState =
  | "queued"
  | "preparing"
  | "running"
  | "finalizing"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "interrupted";

export type ProviderPreference =
  | { kind: "automatic" }
  | { kind: "specific"; providerId: string };

export interface UpscaleJobRequest {
  inputPath: string;
  outputDirectory: string;
  modelId: string;
  modelVariantId: string;
  targetScale: number;
  outputFormat: OutputFormat;
  metadataPolicy: "strip" | "preserveSafe";
  preserveGps: boolean;
  providerPreference: ProviderPreference;
  tileSize: number | null;
  overwrite: boolean;
}

export interface BatchJobRequest {
  inputs: string[];
  defaults: BatchJobDefaults;
}

export interface BatchJobDefaults {
  outputDirectory: string;
  modelId: string;
  modelVariantId: string;
  targetScale: number;
  outputFormat: OutputFormat;
  metadataPolicy: "strip" | "preserveSafe";
  preserveGps: boolean;
  providerPreference: ProviderPreference;
  tileSize: number | null;
  overwrite: boolean;
}

export interface JobSnapshot {
  id: string;
  state: JobState;
  inputPath: string;
  outputPath: string | null;
  previewPath: string | null;
  modelId: string;
  modelPackageVersion: string;
  modelVariantId: string;
  targetScale: number;
  engineId: string;
  providerId: string | null;
  progress: JobProgress | null;
  error: ApiError | null;
  createdAt: string;
  updatedAt: string;
}

export interface JobProgress {
  fraction: number;
  stage: "preparing" | "inference" | "merging" | "resizing" | "encoding";
  completedUnits: number;
  totalUnits: number;
  elapsedSeconds: number;
  estimatedRemainingSeconds: number | null;
}
```

`fraction` is finite and clamped to `[0, 1]`. Progress is monotonic within a job attempt.

### 2.5 Downloads

```typescript
export type ComponentKind = "model" | "runtime" | "application";

export interface DownloadSnapshot {
  id: string;
  componentKind: ComponentKind;
  componentId: string;
  version: string;
  state: "queued" | "downloading" | "verifying" | "installing" | "succeeded" | "failed" | "cancelled";
  downloadedBytes: string;
  totalBytes: string | null;
  error: ApiError | null;
}
```

## 3. Commands

### 3.1 Runtime

```text
get_runtime_status() -> RuntimeStatus
refresh_runtime_status() -> RuntimeStatus
set_provider_preference(preference: ProviderPreference) -> RuntimeStatus
list_runtime_components() -> RuntimeComponentSummary[]
refresh_runtime_catalog() -> RuntimeComponentSummary[]
install_runtime_component(componentId: string, version: string | null) -> DownloadSnapshot
activate_runtime_component(componentId: string, version: string) -> RuntimeStatus
remove_runtime_component(componentId: string, version: string) -> void
```

`get_runtime_status` is read-only and may return cached probe results. `refresh_runtime_status` performs a new local probe. Neither command downloads components. `refresh_runtime_catalog` is an explicit network operation. Runtime activation is transactional, retains the previous working version for rollback, and is rejected while any inference job is active.

### 3.2 Model Catalog and Installation

```text
list_models() -> ModelSummary[]
refresh_model_catalog() -> ModelSummary[]
install_model(modelId: string, packageVersion: string | null) -> DownloadSnapshot
cancel_download(downloadId: string) -> DownloadSnapshot
retry_download(downloadId: string) -> DownloadSnapshot
get_download(downloadId: string) -> DownloadSnapshot
list_downloads() -> DownloadSnapshot[]
verify_model(modelId: string, packageVersion: string | null) -> ModelSummary
activate_model_version(modelId: string, packageVersion: string) -> ModelSummary
remove_model(modelId: string, packageVersion: string) -> void
```

`refresh_model_catalog` is an explicit network operation. `list_models` reads only the local catalog and registry. Installation requires an approved license/provenance state.

### 3.3 Application Updates

```text
check_for_application_update() -> ApplicationUpdate | null
download_application_update(version: string) -> DownloadSnapshot
apply_application_update(downloadId: string) -> void
```

```typescript
export interface ApplicationUpdate {
  version: string;
  releaseNotes: string;
  publishedAt: string;
  downloadSizeBytes: string;
  required: boolean;
}
```

Checking and downloading are network operations. Applying an update requires no active job or component installation and follows the signed Tauri updater policy.

### 3.4 Job Queue

```text
create_upscale_job(request: UpscaleJobRequest) -> JobSnapshot
create_batch_jobs(request: BatchJobRequest) -> JobSnapshot[]
cancel_job(jobId: string) -> JobSnapshot
retry_job(jobId: string) -> JobSnapshot
pause_queue() -> QueueSnapshot
resume_queue() -> QueueSnapshot
get_queue() -> QueueSnapshot
get_job(jobId: string) -> JobSnapshot
list_job_history(cursor: string | null, limit: number) -> JobHistoryPage
remove_job_from_history(jobId: string) -> void
```

`pause_queue` prevents the next queued job from starting; it does not suspend an active inference call. `cancel_job` is idempotent. Batch creation is transactional: either every validated job is queued or none is.

```typescript
export interface QueueSnapshot {
  paused: boolean;
  activeJobId: string | null;
  queuedJobIds: string[];
  revision: string;
}

export interface JobHistoryPage {
  jobs: JobSnapshot[];
  nextCursor: string | null;
}
```

### 3.5 Settings

```text
load_settings() -> AppSettings
save_settings(settings: AppSettings) -> AppSettings
```

```typescript
export interface AppSettings {
  schemaVersion: number;
  outputDirectory: string | null;
  outputFormat: OutputFormat;
  defaultModelId: string | null;
  defaultModelVariantId: string | null;
  defaultTargetScale: number;
  metadataPolicy: "strip" | "preserveSafe";
  preserveGps: boolean;
  providerPreference: ProviderPreference;
  tileSizeOverride: number | null;
  overwriteExisting: boolean;
  locale: string;
  theme: "system" | "light" | "dark";
  checkForUpdates: boolean;
}
```

Settings validation occurs in Rust. Unknown future fields are tolerated during read where safe; unsupported schema versions produce a migration error rather than resetting silently.

### 3.6 Local File Actions

```text
reveal_output(jobId: string) -> void
clear_preview_cache() -> void
```

The frontend does not pass an arbitrary path to a shell-opening command. `reveal_output` resolves a known successful job and opens its parent directory.

## 4. Events

Events are notifications, not the source of truth. The frontend subscribes before creating work and reconciles with snapshot commands after startup, reconnect, or revision gaps.

| Event | Payload |
|---|---|
| `resvera://job-changed` | `JobSnapshot` |
| `resvera://queue-changed` | `QueueSnapshot` |
| `resvera://download-changed` | `DownloadSnapshot` |
| `resvera://runtime-changed` | `RuntimeStatus` |
| `resvera://models-changed` | `{ revision: string }` |

Events may be coalesced. Consumers must not assume delivery of every intermediate progress value.

## 5. Validation Rules

- Input paths must identify regular files in supported formats.
- Output directories must exist or be created only after explicit user selection.
- Input and output canonical paths must not resolve to the same file unless overwrite is explicitly enabled and safe atomic replacement is possible.
- JPEG quality is `1..=100`.
- Lossy WebP quality is `1..=100`; it is `null` for lossless WebP.
- Target scale is restricted to product-supported values.
- Tile size must satisfy the selected model's minimum and alignment rules.
- Model and provider compatibility is checked before a job is committed.
- Unsupported provider preferences fail explicitly; they do not silently mutate stored settings.

## 6. Preview URLs

The backend returns only preview-cache paths to the frontend. The frontend converts them with Tauri's `convertFileSrc`. The asset protocol is scoped to the preview cache and the CSP permits only the required asset origins.

Full-resolution output paths may be displayed as text and used by backend commands, but they are not exposed through a global asset-protocol scope.

```typescript
import { convertFileSrc } from "@tauri-apps/api/core";

export function previewUrl(job: JobSnapshot): string | null {
  return job.previewPath ? convertFileSrc(job.previewPath) : null;
}
```

## 7. Compatibility and Evolution

- Breaking IPC changes require a contract-schema version increase.
- Generated TypeScript must match Rust in CI.
- Persisted job requests include the resolved model package version and engine ID.
- New enum variants must be handled by an explicit frontend fallback.
- A future inference engine adds new engine capabilities and model artifacts without changing existing job command shapes.

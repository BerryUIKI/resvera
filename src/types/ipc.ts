export type OutputFormat =
  | { kind: "sameAsInput" }
  | { kind: "png" }
  | { kind: "jpeg"; quality: number }
  | { kind: "webp"; lossless: boolean; quality: number | null };

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

export interface ApiError {
  code: ErrorCode;
  message: string;
  details: Record<string, unknown> | null;
  retryable: boolean;
}

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
  overwrite: boolean;
  tileSize: number | null;
  providerPreference: string | null;
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
  stage: string;
  completedUnits: number;
  totalUnits: number;
  elapsedSeconds: number;
  estimatedRemainingSeconds: number | null;
}

export interface QueueSnapshot {
  paused: boolean;
  activeJobId: string | null;
  queuedJobIds: string[];
  revision: string;
}

export interface AppSettings {
  schemaVersion: number;
  outputDirectory: string | null;
  outputFormat: OutputFormat;
  defaultModelId: string | null;
  defaultModelVariantId: string | null;
  defaultTargetScale: number;
  namingTemplate: string;
  metadataPolicy: "strip" | "preserveSafe";
  preserveGps: boolean;
  providerPreference: ProviderPreference;
  tileSizeOverride: number | null;
  overwriteExisting: boolean;
  locale: string;
  theme: "system" | "light" | "dark";
  checkForUpdates: boolean;
}

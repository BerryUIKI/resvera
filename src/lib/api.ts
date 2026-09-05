import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import {
  AppSettings,
  JobHistoryPage,
  JobSnapshot,
  ModelSummary,
  QueueSnapshot,
  RuntimeStatus,
  UpscaleJobRequest,
} from "../types/ipc";

export function isTauri(): boolean {
  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);
}

export function resolveImageUrl(pathOrUrl: string | null | undefined): string | null {
  if (!pathOrUrl) return null;
  if (
    pathOrUrl.startsWith("blob:") ||
    pathOrUrl.startsWith("data:") ||
    pathOrUrl.startsWith("http://") ||
    pathOrUrl.startsWith("https://") ||
    pathOrUrl.startsWith("asset://")
  ) {
    return pathOrUrl;
  }
  if (isTauri()) {
    try {
      return convertFileSrc(pathOrUrl);
    } catch {
      return pathOrUrl;
    }
  }
  return pathOrUrl;
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  if (isTauri()) {
    return await invoke<RuntimeStatus>("get_runtime_status");
  }
  return {
    engine: {
      id: "browser-stub",
      displayName: "Browser Preview Mode (Runtime Unavailable)",
      version: "0.0.0",
      healthy: false,
      diagnostic: "Resvera native desktop runtime (Tauri v2 + ONNX Runtime) is required for inference and model execution.",
    },
    providers: [
      {
        id: "cpu",
        displayName: "CPU (Native Desktop Only)",
        version: null,
        installed: false,
        available: false,
        deviceName: null,
        dedicatedMemoryBytes: null,
        diagnostic: "Unavailable in standard browser sandbox",
      },
    ],
    automaticProviderOrder: [],
    offlineReady: false,
  };
}

export async function listModels(): Promise<ModelSummary[]> {
  if (isTauri()) {
    return await invoke<ModelSummary[]>("list_models");
  }
  return [
    {
      id: "realesrgan-x4plus",
      packageVersion: "1.0.0",
      displayName: "Real-ESRGAN x4plus",
      family: "rrdb",
      category: "photo",
      nativeScales: [4],
      installed: false,
      updateAvailable: false,
      downloadSizeBytes: "67051644",
      licenseSpdx: "BSD-3-Clause",
      redistributionReview: "approved",
      validatedProviders: ["cpu", "directml", "coreml"],
      variants: [{ id: "default", nativeScale: 4, strength: null }],
    },
    {
      id: "realesrgan-x4plus-anime",
      packageVersion: "1.0.0",
      displayName: "Real-ESRGAN x4plus Anime (6B)",
      family: "rrdb-6b",
      category: "anime",
      nativeScales: [4],
      installed: false,
      updateAvailable: false,
      downloadSizeBytes: "17939969",
      licenseSpdx: "BSD-3-Clause",
      redistributionReview: "approved",
      validatedProviders: ["cpu", "directml", "coreml"],
      variants: [{ id: "default", nativeScale: 4, strength: null }],
    },
    {
      id: "real-cugan-2x",
      packageVersion: "1.0.0",
      displayName: "Real-CUGAN 2x",
      family: "cugan",
      category: "anime",
      nativeScales: [2],
      installed: false,
      updateAvailable: false,
      downloadSizeBytes: "15204812",
      licenseSpdx: "MIT",
      redistributionReview: "approved",
      validatedProviders: ["cpu", "directml"],
      variants: [
        { id: "no-denoise", nativeScale: 2, strength: "-1" },
        { id: "denoise-1", nativeScale: 2, strength: "1" },
        { id: "denoise-2", nativeScale: 2, strength: "2" },
        { id: "denoise-3", nativeScale: 2, strength: "3" },
      ],
    },
    {
      id: "real-cugan-4x",
      packageVersion: "1.0.0",
      displayName: "Real-CUGAN 4x",
      family: "cugan",
      category: "anime",
      nativeScales: [4],
      installed: false,
      updateAvailable: false,
      downloadSizeBytes: "28145290",
      licenseSpdx: "MIT",
      redistributionReview: "approved",
      validatedProviders: ["cpu", "directml"],
      variants: [
        { id: "no-denoise", nativeScale: 4, strength: "-1" },
        { id: "denoise-3", nativeScale: 4, strength: "3" },
      ],
    },
    {
      id: "real-hat-gan-4x",
      packageVersion: "1.0.0",
      displayName: "Real-HAT-GAN 4x",
      family: "hat",
      category: "photo",
      nativeScales: [4],
      installed: false,
      updateAvailable: false,
      downloadSizeBytes: "76483920",
      licenseSpdx: "Apache-2.0",
      redistributionReview: "approved",
      validatedProviders: ["cpu", "directml", "cuda"],
      variants: [{ id: "default", nativeScale: 4, strength: null }],
    },
  ];
}

export async function loadSettings(): Promise<AppSettings> {
  if (isTauri()) {
    return await invoke<AppSettings>("load_settings");
  }
  return {
    schemaVersion: 1,
    outputDirectory: null,
    modelsDirectory: "~/.resvera/models",
    outputFormat: { kind: "png" },
    defaultModelId: "realesrgan-x4plus",
    defaultModelVariantId: "default",
    defaultTargetScale: 4,
    namingTemplate: "{stem}_{model}_{scale}x",
    metadataPolicy: "preserveSafe",
    preserveGps: false,
    providerPreference: { kind: "automatic" },
    tileSizeOverride: null,
    tileOverlap: 16,
    blendMode: "cosine",
    precision: "fp32",
    gpuDeviceId: 0,
    overwriteExisting: false,
    locale: "zh-CN",
    theme: "dark",
    checkForUpdates: false,
  };
}

export async function saveSettings(settings: AppSettings): Promise<AppSettings> {
  if (isTauri()) {
    return await invoke<AppSettings>("save_settings", { newSettings: settings });
  }
  return settings;
}

export async function createUpscaleJob(req: UpscaleJobRequest): Promise<JobSnapshot> {
  if (isTauri()) {
    return await invoke<JobSnapshot>("create_upscale_job", { req });
  }
  throw new Error("Tauri native runtime required for image upscaling; simulation disabled.");
}

export async function createBatchJobs(req: { inputs: string[]; defaults: any }): Promise<JobSnapshot[]> {
  if (isTauri()) {
    return await invoke<JobSnapshot[]>("create_batch_jobs", { req });
  }
  throw new Error("Tauri native runtime required for batch jobs; simulation disabled.");
}

export async function getJobsHistory(limit = 50): Promise<JobHistoryPage> {
  if (isTauri()) {
    return await invoke<JobHistoryPage>("get_jobs_history", { limit });
  }
  return { jobs: [], nextCursor: null };
}

export async function processNextJob(): Promise<JobSnapshot | null> {
  if (isTauri()) {
    return await invoke<JobSnapshot | null>("process_next_job");
  }
  return null;
}

export async function cancelJob(jobId: string): Promise<JobSnapshot | null> {
  if (isTauri()) {
    return await invoke<JobSnapshot>("cancel_job", { jobId });
  }
  return null;
}

export async function getJob(jobId: string): Promise<JobSnapshot | null> {
  if (isTauri()) {
    return await invoke<JobSnapshot>("get_job", { jobId });
  }
  return null;
}

export async function pauseQueue(): Promise<QueueSnapshot> {
  if (isTauri()) {
    return await invoke<QueueSnapshot>("pause_queue");
  }
  return {
    paused: true,
    activeJobId: null,
    queuedJobIds: [],
    revision: "rev-paused",
  };
}

export async function resumeQueue(): Promise<QueueSnapshot> {
  if (isTauri()) {
    return await invoke<QueueSnapshot>("resume_queue");
  }
  return {
    paused: false,
    activeJobId: null,
    queuedJobIds: [],
    revision: "rev-resumed",
  };
}

export async function getQueue(): Promise<QueueSnapshot> {
  if (isTauri()) {
    return await invoke<QueueSnapshot>("get_queue");
  }
  return {
    paused: false,
    activeJobId: null,
    queuedJobIds: [],
    revision: "rev-1",
  };
}

/**
 * Remove an installed model from disk.
 * Returns `true` if the model directory was found and deleted, `false` if it
 * was already absent (idempotent).  Throws on I/O errors.
 */
export async function uninstallModel(modelId: string): Promise<boolean> {
  if (isTauri()) {
    return await invoke<boolean>("uninstall_model", { modelId });
  }
  // Browser stub: nothing to delete.
  return false;
}

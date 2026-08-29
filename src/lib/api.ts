import { invoke } from "@tauri-apps/api/core";
import {
  AppSettings,
  JobSnapshot,
  ModelSummary,
  QueueSnapshot,
  RuntimeStatus,
  UpscaleJobRequest,
} from "../types/ipc";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  if (isTauri()) {
    return await invoke<RuntimeStatus>("get_runtime_status");
  }
  return {
    engine: {
      id: "ort",
      displayName: "ONNX Runtime (Offline)",
      version: "1.29.0",
      healthy: true,
      diagnostic: null,
    },
    providers: [
      {
        id: "cpu",
        displayName: "CPU (Universal Offline Fallback)",
        version: "1.29.0",
        installed: true,
        available: true,
        deviceName: "Host CPU",
        dedicatedMemoryBytes: null,
        diagnostic: null,
      },
      {
        id: "directml",
        displayName: "DirectML (DirectX 12 GPU)",
        version: "1.29.0",
        installed: true,
        available: true,
        deviceName: "Primary GPU",
        dedicatedMemoryBytes: null,
        diagnostic: null,
      },
    ],
    automaticProviderOrder: ["directml", "cpu"],
    offlineReady: true,
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
      installed: true,
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
      installed: true,
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
      installed: true,
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
      installed: true,
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
      installed: true,
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
    outputFormat: { kind: "png" },
    defaultModelId: "realesrgan-x4plus",
    defaultModelVariantId: "default",
    defaultTargetScale: 4,
    namingTemplate: "{stem}_{model}_{scale}x",
    metadataPolicy: "preserveSafe",
    preserveGps: false,
    providerPreference: { kind: "automatic" },
    tileSizeOverride: null,
    overwriteExisting: false,
    locale: "en-US",
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
  throw new Error("Tauri runtime required for job creation");
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

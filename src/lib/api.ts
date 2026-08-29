import {
  AppSettings,
  ModelSummary,
  RuntimeStatus,
} from "../types/ipc";

// In browser / dev environment or desktop WebView, wrap Tauri invoke or provide robust local runtime
export async function getRuntimeStatus(): Promise<RuntimeStatus> {
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
  ];
}

export async function loadSettings(): Promise<AppSettings> {
  return {
    schemaVersion: 1,
    outputDirectory: null,
    outputFormat: { kind: "png" },
    defaultModelId: "realesrgan-x4plus",
    defaultModelVariantId: "default",
    defaultTargetScale: 4,
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

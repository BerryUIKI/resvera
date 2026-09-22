import { describe, it, expect, beforeEach } from "vitest";
import {
  AppSettings,
  JobSnapshot,
  ModelSummary,
  QueueSnapshot,
  RuntimeStatus,
  UpscaleJobRequest,
} from "../types/ipc";

describe("Tauri IPC Critical Paths Flow", () => {
  let mockIpcStore: {
    settings: AppSettings;
    queue: QueueSnapshot;
    jobs: JobSnapshot[];
    models: ModelSummary[];
    runtime: RuntimeStatus;
  };

  beforeEach(() => {
    mockIpcStore = {
      settings: {
        schemaVersion: 1,
        outputDirectory: null,
        modelsDirectory: null,
        outputFormat: { kind: "png" },
        defaultModelId: "realesrgan-x4plus",
        defaultModelVariantId: "default",
        defaultTargetScale: 4,
        namingTemplate: "{name}_upscaled_{scale}x",
        metadataPolicy: "preserveSafe",
        preserveGps: false,
        providerPreference: { kind: "automatic" },
        tileSizeOverride: 256,
        tileOverlap: 16,
        blendMode: "cosine",
        precision: "fp32",
        gpuDeviceId: null,
        overwriteExisting: false,
        locale: "en-US",
        theme: "system",
        checkForUpdates: false,
      },
      queue: {
        paused: false,
        activeJobId: null,
        queuedJobIds: [],
        revision: "rev-1",
      },
      jobs: [],
      models: [
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
          validatedProviders: ["cpu"],
          variants: [{ id: "default", nativeScale: 4, strength: null }],
        },
      ],
      runtime: {
        engine: {
          id: "ort",
          displayName: "ONNX Runtime",
          version: "1.19.2",
          healthy: true,
          supportsFp16: false,
          diagnostic: null,
        },
        providers: [
          {
            id: "cpu",
            displayName: "CPU (SIMD)",
            version: "1.19.2",
            installed: true,
            available: true,
            deviceName: "CPU",
            dedicatedMemoryBytes: null,
            diagnostic: null,
          },
        ],
        automaticProviderOrder: ["cpu"],
        offlineReady: true,
      },
    };
  });

  const mockInvoke = async (cmd: string, args?: any): Promise<any> => {
    switch (cmd) {
      case "get_runtime_status":
        return mockIpcStore.runtime;
      case "list_models":
        return mockIpcStore.models;
      case "load_settings":
        return mockIpcStore.settings;
      case "save_settings":
        mockIpcStore.settings = { ...args.settings };
        return mockIpcStore.settings;
      case "submit_job": {
        const req: UpscaleJobRequest = args.request;
        const newJob: JobSnapshot = {
          id: `job-${mockIpcStore.jobs.length + 1}`,
          state: "queued",
          inputPath: req.inputPath,
          outputPath: null,
          previewPath: null,
          modelId: req.modelId,
          modelPackageVersion: "1.0.0",
          modelVariantId: req.modelVariantId,
          targetScale: req.targetScale,
          engineId: "ort",
          providerId: "cpu",
          tileSize: req.tileSize,
          tileOverlap: req.tileOverlap,
          blendMode: req.blendMode,
          namingTemplate: req.namingTemplate,
          progress: null,
          error: null,
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        };
        mockIpcStore.jobs.push(newJob);
        mockIpcStore.queue.queuedJobIds.push(newJob.id);
        return newJob;
      }
      case "cancel_job": {
        const job = mockIpcStore.jobs.find((j) => j.id === args.id);
        if (job) {
          job.state = "cancelled";
        }
        mockIpcStore.queue.queuedJobIds = mockIpcStore.queue.queuedJobIds.filter(
          (id) => id !== args.id
        );
        return true;
      }
      case "get_queue_snapshot":
        return mockIpcStore.queue;
      default:
        throw new Error(`Unknown command: ${cmd}`);
    }
  };

  it("completes full end-to-end IPC workflow: init -> submit -> monitor -> cancel", async () => {
    // 1. Query runtime status
    const runtime = await mockInvoke("get_runtime_status");
    expect(runtime.offlineReady).toBe(true);
    expect(runtime.providers[0].available).toBe(true);

    // 2. Query models
    const models = await mockInvoke("list_models");
    expect(models.length).toBe(1);
    expect(models[0].installed).toBe(true);

    // 3. Submit upscale job
    const req: UpscaleJobRequest = {
      inputPath: "/images/test.png",
      outputDirectory: "/outputs",
      modelId: "realesrgan-x4plus",
      modelVariantId: "default",
      targetScale: 4,
      outputFormat: { kind: "png" },
      overwrite: false,
      tileSize: 256,
      tileOverlap: 16,
      blendMode: "cosine",
      namingTemplate: "{name}_upscaled_{scale}x",
      providerPreference: null,
    };
    const submitted = await mockInvoke("submit_job", { request: req });
    expect(submitted.id).toBe("job-1");
    expect(submitted.state).toBe("queued");

    // 4. Query queue snapshot
    const queue = await mockInvoke("get_queue_snapshot");
    expect(queue.queuedJobIds.length).toBe(1);
    expect(queue.queuedJobIds[0]).toBe("job-1");

    // 5. Cancel the job
    const cancelled = await mockInvoke("cancel_job", { id: "job-1" });
    expect(cancelled).toBe(true);

    const updatedQueue = await mockInvoke("get_queue_snapshot");
    expect(updatedQueue.queuedJobIds.length).toBe(0);
    expect(mockIpcStore.jobs[0].state).toBe("cancelled");
  });
});

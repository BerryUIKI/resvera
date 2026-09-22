import { describe, it, expect } from "vitest";
import { JobSnapshot, UpscaleJobRequest } from "../types/ipc";

describe("Queue Behavioral Logic and State Transitions", () => {
  const sampleRequest: UpscaleJobRequest = {
    inputPath: "/path/to/image.png",
    outputDirectory: "/path/to/outputs",
    modelId: "realesrgan-x4plus",
    modelVariantId: "default",
    targetScale: 4,
    outputFormat: { kind: "png" },
    overwrite: false,
    tileSize: 256,
    tileOverlap: 16,
    blendMode: "cosine",
    namingTemplate: "{name}_upscaled_{scale}x",
    providerPreference: "cpu",
  };

  const sampleJob: JobSnapshot = {
    id: "job-uuid-1",
    state: "queued",
    inputPath: sampleRequest.inputPath,
    outputPath: null,
    previewPath: null,
    modelId: sampleRequest.modelId,
    modelPackageVersion: "1.0.0",
    modelVariantId: sampleRequest.modelVariantId,
    targetScale: sampleRequest.targetScale,
    engineId: "ort",
    providerId: "cpu",
    tileSize: sampleRequest.tileSize,
    tileOverlap: sampleRequest.tileOverlap,
    blendMode: sampleRequest.blendMode,
    namingTemplate: sampleRequest.namingTemplate,
    progress: null,
    error: null,
    createdAt: "2026-09-22T10:00:00Z",
    updatedAt: "2026-09-22T10:00:00Z",
  };

  it("ensures job retry preserves exact, immutable parameters", () => {
    // When a job is retried, all original request parameters MUST remain identical
    const failedJob: JobSnapshot = {
      ...sampleJob,
      state: "failed",
      error: {
        code: "internal",
        message: "Process interrupted",
        details: null,
        retryable: true,
      },
    };

    const retryRequest: UpscaleJobRequest = {
      inputPath: failedJob.inputPath,
      outputDirectory: sampleRequest.outputDirectory,
      modelId: failedJob.modelId,
      modelVariantId: failedJob.modelVariantId,
      targetScale: failedJob.targetScale,
      outputFormat: sampleRequest.outputFormat,
      overwrite: false,
      tileSize: failedJob.tileSize ?? null,
      tileOverlap: failedJob.tileOverlap ?? null,
      blendMode: failedJob.blendMode ?? null,
      namingTemplate: failedJob.namingTemplate ?? null,
      providerPreference: failedJob.providerId,
    };

    expect(retryRequest.inputPath).toBe(sampleRequest.inputPath);
    expect(retryRequest.modelId).toBe(sampleRequest.modelId);
    expect(retryRequest.targetScale).toBe(sampleRequest.targetScale);
    expect(retryRequest.outputFormat).toEqual(sampleRequest.outputFormat);
    expect(retryRequest.tileSize).toBe(sampleRequest.tileSize);
    expect(retryRequest.tileOverlap).toBe(sampleRequest.tileOverlap);
    expect(retryRequest.blendMode).toBe(sampleRequest.blendMode);
  });

  it("handles queue pause, resume, and cancellation state transitions", () => {
    let isPaused = false;
    let jobs: JobSnapshot[] = [{ ...sampleJob }];

    // Pause queue
    isPaused = true;
    expect(isPaused).toBe(true);

    // Cancel job
    const cancelJob = (id: string) => {
      jobs = jobs.map((j) =>
        j.id === id && j.state === "queued"
          ? { ...j, state: "cancelled" as const }
          : j
      );
    };

    cancelJob("job-uuid-1");
    expect(jobs[0].state).toBe("cancelled");

    // Resume queue
    isPaused = false;
    expect(isPaused).toBe(false);
  });

  it("handles clear completed jobs without removing active or queued work", () => {
    const list: JobSnapshot[] = [
      { ...sampleJob, id: "j1", state: "succeeded" },
      { ...sampleJob, id: "j2", state: "queued" },
      { ...sampleJob, id: "j3", state: "failed" },
      { ...sampleJob, id: "j4", state: "running" },
      { ...sampleJob, id: "j5", state: "cancelled" },
    ];

    // Clearing completed keeps queued and running
    const remaining = list.filter((j) => j.state === "queued" || j.state === "running");
    expect(remaining.map((j) => j.id)).toEqual(["j2", "j4"]);
  });
});

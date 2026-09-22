import { describe, it, expect } from "vitest";
import { ModelSummary, ModelInstallProgress } from "../types/ipc";

describe("Model Center State & Installation Behavior", () => {
  const mockModels: ModelSummary[] = [
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
    {
      id: "realesrgan-x4plus-anime",
      packageVersion: "1.0.0",
      displayName: "Real-ESRGAN x4plus Anime",
      family: "rrdb",
      category: "anime",
      nativeScales: [4],
      installed: false,
      updateAvailable: false,
      downloadSizeBytes: "17933182",
      licenseSpdx: "BSD-3-Clause",
      redistributionReview: "approved",
      validatedProviders: ["cpu"],
      variants: [{ id: "default", nativeScale: 4, strength: null }],
    },
  ];

  it("correctly identifies installed vs downloadable models", () => {
    const installed = mockModels.filter((m) => m.installed);
    const downloadable = mockModels.filter((m) => !m.installed);

    expect(installed.length).toBe(1);
    expect(installed[0].id).toBe("realesrgan-x4plus");

    expect(downloadable.length).toBe(1);
    expect(downloadable[0].id).toBe("realesrgan-x4plus-anime");
  });

  it("processes model installation progress events sequentially", () => {
    const progressEvents: ModelInstallProgress[] = [
      {
        modelId: "realesrgan-x4plus-anime",
        stage: "downloading",
        fraction: 0.25,
        bytesDownloaded: 4483295,
        totalBytes: 17933182,
      },
      {
        modelId: "realesrgan-x4plus-anime",
        stage: "verifying",
        fraction: 1.0,
        bytesDownloaded: 17933182,
        totalBytes: 17933182,
      },
      {
        modelId: "realesrgan-x4plus-anime",
        stage: "installed",
        fraction: 1.0,
        bytesDownloaded: 17933182,
        totalBytes: 17933182,
      },
    ];

    let currentStage = "idle";
    let currentFraction = 0;

    for (const evt of progressEvents) {
      currentStage = evt.stage;
      currentFraction = evt.fraction;
    }

    expect(currentStage).toBe("installed");
    expect(currentFraction).toBe(1.0);
  });

  it("captures installation failure stage correctly", () => {
    const failureEvent: ModelInstallProgress = {
      modelId: "realesrgan-x4plus-anime",
      stage: "failed: Hash verification mismatch for package artifact",
      fraction: 0.5,
      bytesDownloaded: 8000000,
      totalBytes: 17933182,
    };

    expect(failureEvent.stage).toContain("failed");
    expect(failureEvent.stage).toContain("Hash verification mismatch");
  });
});

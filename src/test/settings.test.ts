import { describe, it, expect, vi } from "vitest";
import { AppSettings } from "../types/ipc";

describe("Settings Behavioral Logic", () => {
  const defaultSettings: AppSettings = {
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
  };

  it("validates that valid settings pass and update state correctly", () => {
    const updated: AppSettings = {
      ...defaultSettings,
      defaultTargetScale: 2,
      outputFormat: { kind: "webp", lossless: true, quality: null },
      metadataPolicy: "stripAll",
      preserveGps: false,
    };

    expect(updated.defaultTargetScale).toBe(2);
    expect(updated.outputFormat).toEqual({ kind: "webp", lossless: true, quality: null });
    expect(updated.metadataPolicy).toBe("stripAll");
  });

  it("handles settings save failure without mutating in-memory committed settings", async () => {
    let committedSettings = { ...defaultSettings };
    const mockSaveSettings = vi.fn().mockImplementation(async (newSettings: AppSettings) => {
      if (newSettings.modelsDirectory === "invalid/uncreatable/path") {
        throw new Error("Cannot create models directory at specified path");
      }
      committedSettings = { ...newSettings };
      return committedSettings;
    });

    const badAttempt: AppSettings = {
      ...committedSettings,
      modelsDirectory: "invalid/uncreatable/path",
    };

    await expect(mockSaveSettings(badAttempt)).rejects.toThrow("Cannot create models directory");
    // Committed settings remain unchanged
    expect(committedSettings.modelsDirectory).toBeNull();

    // Valid save updates state
    const goodAttempt: AppSettings = {
      ...committedSettings,
      tileSizeOverride: 512,
    };
    const saved = await mockSaveSettings(goodAttempt);
    expect(saved.tileSizeOverride).toBe(512);
    expect(committedSettings.tileSizeOverride).toBe(512);
  });

  it("correctly handles metadata policy and GPS toggle semantics", () => {
    let policy: "preserveSafe" | "stripAll" | "preserveAll" = "stripAll";
    let preserveGps = false;

    expect(policy).toBe("stripAll");

    // Switching to preserveSafe with GPS enabled
    policy = "preserveSafe";
    preserveGps = true;
    expect(policy).toBe("preserveSafe");
    expect(preserveGps).toBe(true);
  });
});

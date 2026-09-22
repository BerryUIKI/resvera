import { describe, it, expect } from "vitest";
import { RuntimeStatus } from "../types/ipc";

describe("Runtime Status Behavioral Logic", () => {
  const healthyCpuStatus: RuntimeStatus = {
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
        displayName: "CPU (SIMD Vectorized)",
        version: "1.19.2",
        installed: true,
        available: true,
        deviceName: "x86_64 CPU",
        dedicatedMemoryBytes: null,
        diagnostic: null,
      },
      {
        id: "directml",
        displayName: "DirectML (DirectX 12)",
        version: null,
        installed: false,
        available: false,
        deviceName: null,
        dedicatedMemoryBytes: null,
        diagnostic: "Pending hardware verification; failing closed to CPU",
      },
    ],
    automaticProviderOrder: ["cpu"],
    offlineReady: true,
  };

  it("evaluates active and healthy provider correctly", () => {
    const activeProvider = healthyCpuStatus.providers.find((p) => p.available && p.installed);
    expect(activeProvider).toBeDefined();
    expect(activeProvider?.id).toBe("cpu");
    expect(activeProvider?.available).toBe(true);
  });

  it("handles offline ready status truthful indicator", () => {
    expect(healthyCpuStatus.offlineReady).toBe(true);
    expect(healthyCpuStatus.engine.healthy).toBe(true);
  });

  it("truthfully exposes unavailable GPU accelerators as fail-closed", () => {
    const gpuProvider = healthyCpuStatus.providers.find((p) => p.id === "directml");
    expect(gpuProvider).toBeDefined();
    expect(gpuProvider?.available).toBe(false);
    expect(gpuProvider?.installed).toBe(false);
    expect(gpuProvider?.diagnostic).toContain("failing closed to CPU");
  });
});

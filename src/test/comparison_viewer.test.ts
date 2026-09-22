import { describe, it, expect } from "vitest";
import { normalizePath, resolveImageUrl } from "../lib/api";

describe("Comparison Viewer URL Resolution and Zoom Behavior", () => {
  it("normalizes Windows UNC and extended paths cleanly", () => {
    expect(normalizePath("\\\\?\\C:\\Users\\test\\image.png")).toBe("C:\\Users\\test\\image.png");
    expect(normalizePath("\\\\?\\UNC\\server\\share\\image.png")).toBe("\\\\server\\share\\image.png");
    expect(normalizePath("/regular/unix/path.png")).toBe("/regular/unix/path.png");
  });

  it("resolves blob, data, and web URLs without modification", () => {
    expect(resolveImageUrl("data:image/png;base64,iVBORw")).toBe("data:image/png;base64,iVBORw");
    expect(resolveImageUrl("blob:http://localhost:1420/uuid")).toBe("blob:http://localhost:1420/uuid");
    expect(resolveImageUrl("asset://localhost/C:/path.png")).toBe("asset://localhost/C:/path.png");
    expect(resolveImageUrl(null)).toBeNull();
  });

  it("calculates zoom clamping between 0.5x and 5.0x", () => {
    const clampZoom = (current: number, delta: number) => {
      const factor = delta < 0 ? 0.15 : -0.15;
      return Math.max(0.5, Math.min(5.0, Number((current + factor).toFixed(2))));
    };

    // Zoom in from 1.0
    expect(clampZoom(1.0, -100)).toBe(1.15);

    // Zoom in beyond max clamps to 5.0
    expect(clampZoom(4.95, -100)).toBe(5.0);
    expect(clampZoom(5.0, -100)).toBe(5.0);

    // Zoom out beyond min clamps to 0.5
    expect(clampZoom(0.55, 100)).toBe(0.5);
    expect(clampZoom(0.5, 100)).toBe(0.5);
  });

  it("calculates comparison slider split position bounded between 0 and 100", () => {
    const calcSplit = (clientX: number, rectLeft: number, rectWidth: number) => {
      return Math.max(0, Math.min(100, ((clientX - rectLeft) / rectWidth) * 100));
    };

    const width = 800;
    const left = 100;

    // Middle click
    expect(calcSplit(500, left, width)).toBe(50);

    // Left edge
    expect(calcSplit(100, left, width)).toBe(0);
    expect(calcSplit(50, left, width)).toBe(0); // Clamped

    // Right edge
    expect(calcSplit(900, left, width)).toBe(100);
    expect(calcSplit(950, left, width)).toBe(100); // Clamped
  });
});

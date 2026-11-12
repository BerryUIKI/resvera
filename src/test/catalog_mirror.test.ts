import { describe, it, expect } from "vitest";
// Vite resolves `?raw` at transform time, so the catalog and the mirror module
// are read as text without needing Node globals or `@types/node`.
import apiSource from "../lib/api.ts?raw";
import catalogSource from "../../crates/resvera-models/src/catalogs/production.json?raw";

/**
 * The browser-preview catalog in src/lib/api.ts is a hand-maintained mirror of
 * the signed catalog. Nothing linked the two, so they drifted: the stub kept
 * stale artifact sizes after the models were re-exported, and would still have
 * said "no published package" after artifacts were published.
 *
 * This test reads the real catalog and fails when the mirror disagrees, so the
 * drift shows up here instead of in the UI.
 */

interface CatalogModel {
  id: string;
  size_bytes: number;
  download_urls: string[];
  validated_providers: string[];
  variants: { id: string; native_scale: number; strength: string | null }[];
}

interface Catalog {
  models: CatalogModel[];
}

/**
 * Extracts and evaluates the BROWSER_CATALOG literal from the API module.
 *
 * Evaluated rather than string-matched so the assertion follows the real values;
 * the slice is bounded to the literal so unrelated module code cannot leak in.
 */
function loadBrowserCatalog(): Record<string, unknown>[] {
  const start = apiSource.indexOf("const BROWSER_CATALOG");
  if (start === -1) throw new Error("BROWSER_CATALOG not found in src/lib/api.ts");
  const assign = apiSource.indexOf("=", start);
  const end = apiSource.indexOf("\n];", assign);
  if (end === -1) throw new Error("could not find the end of BROWSER_CATALOG");
  const literal = apiSource.slice(assign + 1, end + 2);
  return new Function(`return ${literal}`)() as Record<string, unknown>[];
}

describe("Browser catalog mirror stays in sync with the signed catalog", () => {
  const catalog = JSON.parse(catalogSource) as Catalog;
  const mirror = loadBrowserCatalog();

  it("mirrors exactly the models in the production catalog", () => {
    const catalogIds = catalog.models.map((m) => m.id).sort();
    const mirrorIds = mirror.map((m) => m.id as string).sort();
    expect(mirrorIds).toEqual(catalogIds);
  });

  it("uses the catalog's artifact sizes", () => {
    for (const entry of catalog.models) {
      const mirrored = mirror.find((m) => m.id === entry.id);
      expect(mirrored, `missing ${entry.id}`).toBeTruthy();
      expect(
        String(mirrored!.downloadSizeBytes),
        `${entry.id}: downloadSizeBytes drifted from the catalog`,
      ).toBe(String(entry.size_bytes));
    }
  });

  it("reports installable exactly when a download URL is published", () => {
    for (const entry of catalog.models) {
      const mirrored = mirror.find((m) => m.id === entry.id);
      expect(
        mirrored!.installable,
        `${entry.id}: installable must equal (download_urls is non-empty)`,
      ).toBe(entry.download_urls.length > 0);
    }
  });

  it("uses the catalog's validated providers", () => {
    for (const entry of catalog.models) {
      const mirrored = mirror.find((m) => m.id === entry.id);
      expect(
        mirrored!.validatedProviders,
        `${entry.id}: validatedProviders drifted`,
      ).toEqual(entry.validated_providers);
    }
  });

  it("uses the catalog's variants", () => {
    for (const entry of catalog.models) {
      const mirrored = mirror.find((m) => m.id === entry.id);
      const expected = entry.variants.map((v) => ({
        id: v.id,
        nativeScale: v.native_scale,
        strength: v.strength,
      }));
      expect(mirrored!.variants, `${entry.id}: variants drifted`).toEqual(expected);
    }
  });
});

import { Component, createSignal } from "solid-js";
import { useI18n } from "../i18n";
import { DropZone } from "./DropZone";
import { resolveImageUrl } from "../lib/api";

interface ComparisonViewerProps {
  beforeUrl: string | null;
  afterUrl: string | null;
  isProcessing?: boolean;
  progressPercent?: number;
  progressStage?: string;
  onFilesSelected?: (files: File[]) => void;
}

export const ComparisonViewer: Component<ComparisonViewerProps> = (props) => {
  const { t } = useI18n();
  const [splitPos, setSplitPos] = createSignal(50);
  const [zoom, setZoom] = createSignal(1);

  const resolvedBefore = () => resolveImageUrl(props.beforeUrl);
  const resolvedAfter = () => resolveImageUrl(props.afterUrl);
  const [isDraggingSlider, setIsDraggingSlider] = createSignal(false);

  const handlePointerMove = (e: MouseEvent | TouchEvent) => {
    if (!resolvedAfter()) return;
    const clientX = "touches" in e ? e.touches[0].clientX : (e as MouseEvent).clientX;
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    if (rect.width > 0) {
      const pos = Math.max(0, Math.min(100, ((clientX - rect.left) / rect.width) * 100));
      setSplitPos(pos);
    }
  };

  return (
    <div class="relative w-full h-full flex flex-col items-center justify-center bg-slate-950/60 overflow-hidden border border-slate-800/80 rounded-2xl">
      {resolvedBefore() ? (
        <div class="relative w-full h-full flex items-center justify-center overflow-hidden p-2">
          {/* Main Image Container with Drag Interaction */}
          <div
            onMouseDown={() => setIsDraggingSlider(true)}
            onMouseUp={() => setIsDraggingSlider(false)}
            onMouseLeave={() => setIsDraggingSlider(false)}
            onMouseMove={(e) => {
              if (isDraggingSlider() || e.buttons === 1) {
                handlePointerMove(e);
              }
            }}
            onTouchMove={handlePointerMove}
            class={`relative max-w-full max-h-full transition-transform duration-75 flex items-center justify-center select-none ${
              resolvedAfter() ? "cursor-ew-resize" : ""
            }`}
            style={{ transform: `scale(${zoom()})` }}
          >
            {/* After Image (Full background or overlay) */}
            <img
              src={resolvedAfter() || resolvedBefore() || ""}
              alt="After"
              class={`max-w-[70vw] max-h-[65vh] object-contain rounded-xl select-none shadow-2xl pointer-events-none ${
                resolvedAfter() ? "filter contrast-105" : ""
              }`}
            />

            {/* Before Image (Clipped overlay) */}
            {resolvedAfter() && (
              <div
                class="absolute inset-0 overflow-hidden flex items-center justify-center pointer-events-none"
                style={{ "clip-path": `polygon(0 0, ${splitPos()}% 0, ${splitPos()}% 100%, 0 100%)` }}
              >
                <img
                  src={resolvedBefore() || ""}
                  alt="Before"
                  class="max-w-[70vw] max-h-[65vh] object-contain rounded-xl select-none pointer-events-none"
                />
              </div>
            )}

            {/* Slider Divider Line */}
            {resolvedAfter() && (
              <div
                class="absolute top-0 bottom-0 w-0.5 bg-sky-400 shadow-xl pointer-events-none"
                style={{ left: `${splitPos()}%` }}
              >
                <div class="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-7 h-7 bg-sky-500 rounded-full flex items-center justify-center text-slate-950 text-xs font-bold shadow-lg border-2 border-white/80">
                  ↔
                </div>
              </div>
            )}
          </div>

          {/* Top Banner when Upscaled Image is Ready */}
          {resolvedAfter() && (
            <div class="absolute top-4 left-4 flex items-center space-x-2 bg-emerald-950/80 backdrop-blur-md px-3 py-1.5 rounded-xl border border-emerald-700/80 text-emerald-400 text-xs shadow-lg">
              <span>✨</span>
              <span class="font-semibold">超分辨率增强完成 (可拖动画布左右对比)</span>
            </div>
          )}

          {/* Processing State HUD Overlay */}
          {props.isProcessing && (
            <div class="absolute inset-0 z-20 flex flex-col items-center justify-center bg-slate-950/70 backdrop-blur-sm">
              <div class="bg-slate-900 border border-slate-700/80 rounded-2xl p-6 shadow-2xl flex flex-col items-center space-y-4 max-w-sm w-full">
                <div class="w-12 h-12 rounded-full border-4 border-sky-400 border-t-transparent animate-spin"></div>
                <div class="text-center">
                  <h4 class="text-sm font-semibold text-slate-100 mb-1">
                    {props.progressStage || t("queue.processing")}
                  </h4>
                  <p class="text-xs text-slate-400">Offline inference running on local accelerator...</p>
                </div>
                <div class="w-full bg-slate-800 rounded-full h-2 overflow-hidden">
                  <div
                    class="bg-sky-500 h-full rounded-full transition-all duration-200"
                    style={{ width: `${props.progressPercent || 0}%` }}
                  ></div>
                </div>
                <span class="text-xs font-mono text-sky-400 font-semibold">
                  {props.progressPercent || 0}%
                </span>
              </div>
            </div>
          )}

          {/* Range Slider for Split */}
          {resolvedAfter() && (
            <div class="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center space-x-3 bg-slate-900/90 backdrop-blur-md px-5 py-2.5 rounded-full border border-slate-700 shadow-2xl z-10">
              <span class="text-xs text-slate-400 font-semibold">{t("viewer.before")}</span>
              <input
                type="range"
                min="0"
                max="100"
                value={splitPos()}
                onInput={(e) => setSplitPos(Number(e.currentTarget.value))}
                class="w-56 h-1.5 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-sky-400"
              />
              <span class="text-xs text-sky-400 font-semibold">{t("viewer.after")}</span>
            </div>
          )}

          {/* Zoom controls */}
          <div class="absolute top-4 right-4 flex items-center space-x-1 bg-slate-900/90 backdrop-blur px-2.5 py-1.5 rounded-xl border border-slate-700 text-xs shadow-lg">
            <button
              onClick={() => setZoom((z) => Math.max(0.5, z - 0.25))}
              class="px-2 py-1 hover:bg-slate-800 rounded-lg text-slate-300 font-bold transition"
            >
              -
            </button>
            <span class="px-2 text-slate-300 font-mono font-medium">{Math.round(zoom() * 100)}%</span>
            <button
              onClick={() => setZoom((z) => Math.min(3.0, z + 0.25))}
              class="px-2 py-1 hover:bg-slate-800 rounded-lg text-slate-300 font-bold transition"
            >
              +
            </button>
          </div>
        </div>
      ) : (
        <DropZone onFilesSelected={(files) => props.onFilesSelected?.(files)} />
      )}
    </div>
  );
};

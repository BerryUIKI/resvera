import { Component, createEffect, createSignal, Show } from "solid-js";
import { useI18n } from "../i18n";
import { DropZone } from "./DropZone";
import { readImageData, resolveImageUrl } from "../lib/api";

interface ComparisonViewerProps {
  beforeUrl: string | null;
  afterUrl: string | null;
  isProcessing?: boolean;
  progressPercent?: number;
  progressStage?: string;
  onFilesSelected?: (files: File[]) => void;
  onPickImages?: () => void;
}

export const ComparisonViewer: Component<ComparisonViewerProps> = (props) => {
  const { t } = useI18n();
  const [splitPos, setSplitPos] = createSignal(50);
  const [zoom, setZoom] = createSignal(1);
  const [panX, setPanX] = createSignal(0);
  const [panY, setPanY] = createSignal(0);

  const [beforeSrc, setBeforeSrc] = createSignal<string | null>(null);
  const [afterSrc, setAfterSrc] = createSignal<string | null>(null);

  const [isPanning, setIsPanning] = createSignal(false);
  const [isDraggingDivider, setIsDraggingDivider] = createSignal(false);
  const [dragStart, setDragStart] = createSignal({ x: 0, y: 0, initialPanX: 0, initialPanY: 0 });

  let imageContainerRef: HTMLDivElement | undefined;

  let lastFailedBeforeUrl: string | null = null;
  let lastFailedAfterUrl: string | null = null;

  createEffect(() => {
    const raw = props.beforeUrl;
    lastFailedBeforeUrl = null;
    if (!raw) {
      setBeforeSrc(null);
      setZoom(1);
      setPanX(0);
      setPanY(0);
      setSplitPos(50);
      return;
    }
    const resolved = resolveImageUrl(raw);
    setBeforeSrc(resolved);
  });

  createEffect(() => {
    const raw = props.afterUrl;
    lastFailedAfterUrl = null;
    if (!raw) {
      setAfterSrc(null);
      return;
    }
    const resolved = resolveImageUrl(raw);
    setAfterSrc(resolved);
  });

  const handleBeforeError = async () => {
    const raw = props.beforeUrl;
    if (raw && raw !== lastFailedBeforeUrl && !raw.startsWith("data:") && !raw.startsWith("blob:")) {
      lastFailedBeforeUrl = raw;
      try {
        const b64 = await readImageData(raw);
        setBeforeSrc(b64);
      } catch (e) {
        console.warn("Failed to load before image via IPC fallback:", e);
      }
    }
  };

  const handleAfterError = async () => {
    const raw = props.afterUrl;
    if (raw && raw !== lastFailedAfterUrl && !raw.startsWith("data:") && !raw.startsWith("blob:")) {
      lastFailedAfterUrl = raw;
      try {
        const b64 = await readImageData(raw);
        setAfterSrc(b64);
      } catch (e) {
        console.warn("Failed to load after image via IPC fallback:", e);
      }
    }
  };

  const resolvedBefore = () => beforeSrc();
  const resolvedAfter = () => afterSrc();

  const handleResetZoom = () => {
    setZoom(1);
    setPanX(0);
    setPanY(0);
  };

  const handleWheel = (e: WheelEvent) => {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 0.15 : -0.15;
    setZoom((z) => {
      const next = Math.max(0.5, Math.min(5.0, Number((z + factor).toFixed(2))));
      if (next <= 1) {
        setPanX(0);
        setPanY(0);
      }
      return next;
    });
  };

  const handleMouseDown = (e: MouseEvent) => {
    if (isDraggingDivider()) return;
    if (e.button === 0 || e.button === 1) {
      setIsPanning(true);
      setDragStart({
        x: e.clientX,
        y: e.clientY,
        initialPanX: panX(),
        initialPanY: panY(),
      });
    }
  };

  const handleMouseMove = (e: MouseEvent) => {
    if (isDraggingDivider() && imageContainerRef) {
      const rect = imageContainerRef.getBoundingClientRect();
      if (rect.width > 0) {
        const pos = Math.max(0, Math.min(100, ((e.clientX - rect.left) / rect.width) * 100));
        setSplitPos(pos);
      }
      return;
    }

    if (isPanning()) {
      const dx = e.clientX - dragStart().x;
      const dy = e.clientY - dragStart().y;
      setPanX(dragStart().initialPanX + dx);
      setPanY(dragStart().initialPanY + dy);
    }
  };

  const handleMouseUp = () => {
    setIsPanning(false);
    setIsDraggingDivider(false);
  };

  const handleDividerMouseDown = (e: MouseEvent) => {
    e.stopPropagation();
    setIsDraggingDivider(true);
  };

  return (
    <div
      class="relative w-full h-full flex flex-col items-center justify-center bg-slate-950/60 overflow-hidden border border-slate-800/80 rounded-2xl select-none"
      onWheel={handleWheel}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
    >
      {resolvedBefore() ? (
        <div
          class="relative w-full h-full flex items-center justify-center overflow-hidden p-2"
          onMouseDown={handleMouseDown}
        >
          {/* Main Transformed Image Canvas Container */}
          <div
            ref={imageContainerRef}
            class={`relative max-w-full max-h-full transition-transform duration-75 flex items-center justify-center ${
              isPanning() ? "cursor-grabbing" : zoom() > 1 ? "cursor-grab" : "cursor-default"
            }`}
            style={{
              transform: `translate(${panX()}px, ${panY()}px) scale(${zoom()})`,
            }}
          >
            {/* After Image (Full background) */}
            <img
              src={resolvedAfter() || resolvedBefore() || ""}
              alt="After"
              onError={handleAfterError}
              draggable={false}
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
                  onError={handleBeforeError}
                  draggable={false}
                  class="max-w-[70vw] max-h-[65vh] object-contain rounded-xl select-none pointer-events-none"
                />
              </div>
            )}

            {/* Interactive Split Divider Handle */}
            {resolvedAfter() && (
              <div
                class="absolute top-0 bottom-0 w-1 bg-sky-400/90 shadow-xl cursor-ew-resize z-10 flex items-center justify-center"
                style={{ left: `${splitPos()}%`, "margin-left": "-2px" }}
                onMouseDown={handleDividerMouseDown}
              >
                <div class="w-7 h-7 bg-sky-500 hover:bg-sky-400 rounded-full flex items-center justify-center text-slate-950 text-xs font-bold shadow-lg border-2 border-white/90 active:scale-95 transition-transform cursor-ew-resize">
                  ↔
                </div>
              </div>
            )}
          </div>

          {/* Top Banner when Upscaled Image is Ready */}
          {resolvedAfter() && (
            <div class="absolute top-4 left-4 flex items-center space-x-2 bg-emerald-950/80 backdrop-blur-md px-3 py-1.5 rounded-xl border border-emerald-700/80 text-emerald-400 text-xs shadow-lg pointer-events-none">
              <span>✨</span>
              <span class="font-semibold">{t("viewer.completedHint")}</span>
            </div>
          )}

          {/* Processing State HUD Overlay */}
          {props.isProcessing && (
            <div class="absolute inset-0 z-20 flex flex-col items-center justify-center bg-slate-950/70 backdrop-blur-sm pointer-events-none">
              <div class="bg-slate-900 border border-slate-700/80 rounded-2xl p-6 shadow-2xl flex flex-col items-center space-y-4 max-w-sm w-full">
                <div class="w-12 h-12 rounded-full border-4 border-sky-400 border-t-transparent animate-spin"></div>
                <div class="text-center">
                  <h4 class="text-sm font-semibold text-slate-100 mb-1">
                    {props.progressStage || t("queue.processing")}
                  </h4>
                  <p class="text-xs text-slate-400">{t("viewer.processingHint")}</p>
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

          {/* Zoom and Fit Controls */}
          <div class="absolute top-4 right-4 flex items-center space-x-1 bg-slate-900/90 backdrop-blur px-2.5 py-1.5 rounded-xl border border-slate-700 text-xs shadow-lg z-10">
            <button
              onClick={() => {
                setZoom((z) => {
                  const next = Math.max(0.5, Number((z - 0.25).toFixed(2)));
                  if (next <= 1) {
                    setPanX(0);
                    setPanY(0);
                  }
                  return next;
                });
              }}
              title="Zoom out"
              class="w-6 h-6 flex items-center justify-center hover:bg-slate-800 rounded-lg text-slate-300 font-bold transition cursor-pointer"
            >
              -
            </button>
            <button
              onClick={handleResetZoom}
              title="Reset Zoom / Fit"
              class="px-2 py-0.5 hover:bg-slate-800 rounded-lg text-slate-300 font-mono font-medium transition cursor-pointer"
            >
              {Math.round(zoom() * 100)}%
            </button>
            <button
              onClick={() => setZoom((z) => Math.min(5.0, Number((z + 0.25).toFixed(2))))}
              title="Zoom in"
              class="w-6 h-6 flex items-center justify-center hover:bg-slate-800 rounded-lg text-slate-300 font-bold transition cursor-pointer"
            >
              +
            </button>
            <Show when={zoom() !== 1 || panX() !== 0 || panY() !== 0}>
              <button
                onClick={handleResetZoom}
                title={t("viewer.fit")}
                class="ml-1 px-1.5 py-0.5 bg-sky-950/80 hover:bg-sky-900 border border-sky-800 text-sky-400 text-[11px] rounded-lg font-medium transition cursor-pointer"
              >
                {t("viewer.fit")}
              </button>
            </Show>
          </div>
        </div>
      ) : (
        <DropZone
          onFilesSelected={(files) => props.onFilesSelected?.(files)}
          onPickImages={props.onPickImages}
        />
      )}
    </div>
  );
};

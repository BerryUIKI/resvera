import { Component, createSignal } from "solid-js";

interface ComparisonViewerProps {
  beforeUrl: string | null;
  afterUrl: string | null;
}

export const ComparisonViewer: Component<ComparisonViewerProps> = (props) => {
  const [splitPos, setSplitPos] = createSignal(50);
  const [zoom, setZoom] = createSignal(1);

  return (
    <div class="relative w-full h-full flex flex-col items-center justify-center bg-slate-950/60 overflow-hidden border border-slate-800/80 rounded-xl">
      {props.beforeUrl ? (
        <div class="relative w-full h-full flex items-center justify-center overflow-hidden">
          {/* Main Image Container */}
          <div
            class="relative max-w-full max-h-full transition-transform duration-75"
            style={{ transform: `scale(${zoom()})` }}
          >
            {/* After Image (Full background or overlay) */}
            <img
              src={props.afterUrl || props.beforeUrl}
              alt="After"
              class="max-w-[70vw] max-h-[60vh] object-contain rounded select-none pointer-events-none"
            />

            {/* Before Image (Clipped overlay) */}
            {props.afterUrl && (
              <div
                class="absolute inset-0 overflow-hidden"
                style={{ "clip-path": `polygon(0 0, ${splitPos()}% 0, ${splitPos()}% 100%, 0 100%)` }}
              >
                <img
                  src={props.beforeUrl}
                  alt="Before"
                  class="max-w-[70vw] max-h-[60vh] object-contain rounded select-none pointer-events-none"
                />
              </div>
            )}

            {/* Slider Divider Line */}
            {props.afterUrl && (
              <div
                class="absolute top-0 bottom-0 w-0.5 bg-sky-400 shadow-lg pointer-events-none"
                style={{ left: `${splitPos()}%` }}
              >
                <div class="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-6 h-6 bg-sky-500 rounded-full flex items-center justify-center text-slate-950 text-xs font-bold shadow-md">
                  ↔
                </div>
              </div>
            )}
          </div>

          {/* Range Slider for Split */}
          {props.afterUrl && (
            <div class="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center space-x-3 bg-slate-900/90 backdrop-blur px-4 py-2 rounded-full border border-slate-700 shadow-lg">
              <span class="text-xs text-slate-400 font-medium">Before</span>
              <input
                type="range"
                min="0"
                max="100"
                value={splitPos()}
                onInput={(e) => setSplitPos(Number(e.currentTarget.value))}
                class="w-48 h-1.5 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-sky-400"
              />
              <span class="text-xs text-slate-400 font-medium">After</span>
            </div>
          )}

          {/* Zoom controls */}
          <div class="absolute top-4 right-4 flex items-center space-x-1 bg-slate-900/80 backdrop-blur px-2 py-1 rounded-lg border border-slate-700 text-xs">
            <button
              onClick={() => setZoom((z) => Math.max(0.5, z - 0.25))}
              class="px-2 py-1 hover:bg-slate-800 rounded text-slate-300 font-bold"
            >
              -
            </button>
            <span class="px-2 text-slate-300 font-mono">{Math.round(zoom() * 100)}%</span>
            <button
              onClick={() => setZoom((z) => Math.min(3.0, z + 0.25))}
              class="px-2 py-1 hover:bg-slate-800 rounded text-slate-300 font-bold"
            >
              +
            </button>
          </div>
        </div>
      ) : (
        <div class="flex flex-col items-center justify-center p-8 text-center text-slate-500">
          <svg class="w-16 h-16 mb-4 text-slate-700" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          <p class="text-sm font-medium text-slate-400">Select an image to preview comparison</p>
          <p class="text-xs text-slate-600 mt-1">PNG, JPEG, and WebP supported locally</p>
        </div>
      )}
    </div>
  );
};

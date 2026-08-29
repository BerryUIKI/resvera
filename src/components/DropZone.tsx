import { Component, createSignal } from "solid-js";
import { useI18n } from "../i18n";

interface DropZoneProps {
  onFilesSelected: (files: File[]) => void;
}

export const DropZone: Component<DropZoneProps> = (props) => {
  const { t } = useI18n();
  const [isDragging, setIsDragging] = createSignal(false);

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(true);
  };

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);

    if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const validFiles = Array.from(e.dataTransfer.files).filter((f) =>
        f.type.startsWith("image/")
      );
      if (validFiles.length > 0) {
        props.onFilesSelected(validFiles);
      }
    }
  };

  const handleFileInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    if (target.files && target.files.length > 0) {
      props.onFilesSelected(Array.from(target.files));
    }
  };

  // Helper to generate instant low-res test canvas images for demonstrating dramatic 4x AI super-resolution
  const loadSampleImage = (type: "photo" | "anime") => {
    const canvas = document.createElement("canvas");
    canvas.width = 160;
    canvas.height = 160;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    if (type === "photo") {
      // Create landscape gradient & sun
      const grad = ctx.createLinearGradient(0, 0, 0, 160);
      grad.addColorStop(0, "#0284c7");
      grad.addColorStop(0.5, "#38bdf8");
      grad.addColorStop(1, "#0f172a");
      ctx.fillStyle = grad;
      ctx.fillRect(0, 0, 160, 160);

      // Sun
      ctx.fillStyle = "#fde047";
      ctx.beginPath();
      ctx.arc(120, 45, 18, 0, Math.PI * 2);
      ctx.fill();

      // Mountains
      ctx.fillStyle = "#1e293b";
      ctx.beginPath();
      ctx.moveTo(0, 160);
      ctx.lineTo(50, 85);
      ctx.lineTo(100, 160);
      ctx.fill();

      ctx.fillStyle = "#334155";
      ctx.beginPath();
      ctx.moveTo(60, 160);
      ctx.lineTo(120, 65);
      ctx.lineTo(160, 160);
      ctx.fill();

      // Detailed text
      ctx.fillStyle = "#ffffff";
      ctx.font = "bold 11px sans-serif";
      ctx.fillText("Resvera Photo Demo", 18, 35);
    } else {
      // Create vibrant anime gradient & character circle
      const grad = ctx.createLinearGradient(0, 0, 160, 160);
      grad.addColorStop(0, "#ec4899");
      grad.addColorStop(0.5, "#8b5cf6");
      grad.addColorStop(1, "#3b82f6");
      ctx.fillStyle = grad;
      ctx.fillRect(0, 0, 160, 160);

      // Anime character badge
      ctx.fillStyle = "#ffffff";
      ctx.beginPath();
      ctx.arc(80, 75, 32, 0, Math.PI * 2);
      ctx.fill();

      ctx.fillStyle = "#ec4899";
      ctx.font = "bold 20px sans-serif";
      ctx.textAlign = "center";
      ctx.fillText("🎨", 80, 82);

      ctx.fillStyle = "#ffffff";
      ctx.font = "bold 11px sans-serif";
      ctx.fillText("Resvera Anime Demo", 80, 135);
    }

    canvas.toBlob((blob) => {
      if (blob) {
        const file = new File(
          [blob],
          type === "photo" ? "sample_landscape_photo.png" : "sample_anime_art.png",
          { type: "image/png" }
        );
        props.onFilesSelected([file]);
      }
    }, "image/png");
  };

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      class={`w-full h-full flex flex-col items-center justify-center border-2 border-dashed rounded-2xl p-8 transition select-none ${
        isDragging()
          ? "border-sky-400 bg-sky-950/30 scale-[0.99]"
          : "border-slate-800 bg-slate-900/30 hover:border-slate-700"
      }`}
    >
      <div class="w-16 h-16 rounded-2xl bg-sky-500/10 border border-sky-500/30 flex items-center justify-center text-sky-400 mb-4 shadow-lg shadow-sky-500/5">
        <svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.75"
            d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
          />
        </svg>
      </div>

      <h3 class="text-sm font-semibold text-slate-200 mb-1">
        {t("queue.dropImages")}
      </h3>
      <p class="text-xs text-slate-500 mb-6 text-center max-w-sm">
        Supports PNG, JPEG, and WebP images. All processing is 100% offline.
      </p>

      <div class="flex items-center space-x-3">
        <label class="px-5 py-2.5 bg-sky-500 hover:bg-sky-400 text-slate-950 font-semibold text-xs rounded-xl cursor-pointer shadow-lg shadow-sky-500/20 transition flex items-center space-x-2">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          <span>{t("controls.addImages")}</span>
          <input
            type="file"
            multiple
            accept="image/png,image/jpeg,image/webp"
            onChange={handleFileInput}
            class="hidden"
          />
        </label>
      </div>

      {/* Quick Test Samples */}
      <div class="mt-8 pt-6 border-t border-slate-800/80 flex flex-col items-center">
        <span class="text-[11px] font-medium text-slate-400 mb-3">
          Or try instant sample demo images:
        </span>
        <div class="flex items-center space-x-3">
          <button
            onClick={() => loadSampleImage("photo")}
            class="px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 text-xs flex items-center space-x-2 transition shadow-sm"
          >
            <span>📸</span>
            <span>Landscape Photo Demo</span>
          </button>
          <button
            onClick={() => loadSampleImage("anime")}
            class="px-3.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 text-xs flex items-center space-x-2 transition shadow-sm"
          >
            <span>🎨</span>
            <span>Anime Illustration Demo</span>
          </button>
        </div>
      </div>
    </div>
  );
};

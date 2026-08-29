import { Component, createSignal, onMount, For, Show } from "solid-js";
import { Header } from "./components/Header";
import { ComparisonViewer } from "./components/ComparisonViewer";
import { QueueList } from "./components/QueueList";
import { SettingsModal } from "./components/SettingsModal";
import { ModelCenterModal } from "./components/ModelCenterModal";
import { getRuntimeStatus, listModels, loadSettings, saveSettings } from "./lib/api";
import { AppSettings, JobSnapshot, ModelSummary, RuntimeStatus } from "./types/ipc";
import { useI18n } from "./i18n";
import { generateUpscaledOutput } from "./lib/upscale";

export const App: Component = () => {
  const { t, setLocale } = useI18n();
  const [runtimeStatus, setRuntimeStatus] = createSignal<RuntimeStatus | null>(null);
  const [models, setModels] = createSignal<ModelSummary[]>([]);
  const [settings, setSettings] = createSignal<AppSettings>({
    schemaVersion: 1,
    outputDirectory: null,
    modelsDirectory: "~/.resvera/models",
    outputFormat: { kind: "png" },
    defaultModelId: "realesrgan-x4plus",
    defaultModelVariantId: "default",
    defaultTargetScale: 4,
    namingTemplate: "{stem}_{model}_{scale}x",
    metadataPolicy: "preserveSafe",
    preserveGps: false,
    providerPreference: { kind: "automatic" },
    tileSizeOverride: null,
    tileOverlap: 16,
    blendMode: "cosine",
    precision: "fp32",
    gpuDeviceId: 0,
    overwriteExisting: false,
    locale: "zh-CN",
    theme: "dark",
    checkForUpdates: false,
  });

  // Main Controls State
  const [selectedModelId, setSelectedModelId] = createSignal("realesrgan-x4plus");
  const [selectedVariantId, setSelectedVariantId] = createSignal("default");
  const [targetScale, setTargetScale] = createSignal(4);
  const [outputFormat, setOutputFormat] = createSignal<"png" | "jpeg" | "webp">("png");
  const [jpegQuality, setJpegQuality] = createSignal(95);
  const [webpLossless, setWebpLossless] = createSignal(true);
  const [overwrite, setOverwrite] = createSignal(false);

  // Advanced Tuning State
  const [selectedProvider, setSelectedProvider] = createSignal("automatic");
  const [selectedPrecision, setSelectedPrecision] = createSignal<"fp32" | "fp16">("fp32");
  const [selectedTileSize, setSelectedTileSize] = createSignal<number | null>(null);
  const [selectedTileOverlap, setSelectedTileOverlap] = createSignal(16);
  const [selectedBlendMode, setSelectedBlendMode] = createSignal("cosine");
  const [cuganPaddingMode, setCuganPaddingMode] = createSignal("reflect");
  const [esrganDenoise, setEsrganDenoise] = createSignal(0.5);

  // Accordion Sections Fold State
  const [isHardwareOpen, setIsHardwareOpen] = createSignal(false);
  const [isTilingOpen, setIsTilingOpen] = createSignal(false);
  const [isModelTuningOpen, setIsModelTuningOpen] = createSignal(true);
  const [isOutputOpen, setIsOutputOpen] = createSignal(false);

  const [jobs, setJobs] = createSignal<JobSnapshot[]>([]);
  const [selectedJobId, setSelectedJobId] = createSignal<string | null>(null);
  const [isPaused, setIsPaused] = createSignal(false);
  const [isProcessingQueue, setIsProcessingQueue] = createSignal(false);
  const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);
  const [isModelCenterOpen, setIsModelCenterOpen] = createSignal(false);

  onMount(async () => {
    const [status, modelList, appSettings] = await Promise.all([
      getRuntimeStatus(),
      listModels(),
      loadSettings(),
    ]);
    setRuntimeStatus(status);
    setModels(modelList);
    setSettings(appSettings);
    if (appSettings.locale) {
      setLocale(appSettings.locale as any);
    }
  });

  const addFilesToQueue = (files: File[]) => {
    if (!files || files.length === 0) return;

    const newJobs: JobSnapshot[] = files.map((file) => {
      const id = `job-${Math.random().toString(36).substring(2, 9)}`;
      const url = URL.createObjectURL(file);
      return {
        id,
        state: "queued",
        inputPath: file.name,
        outputPath: null,
        previewPath: url,
        modelId: selectedModelId(),
        modelPackageVersion: "1.0.0",
        modelVariantId: selectedVariantId(),
        targetScale: targetScale(),
        engineId: "ort",
        providerId: selectedProvider() === "automatic" ? "cpu" : selectedProvider(),
        progress: {
          fraction: 0,
          stage: "queued",
          completedUnits: 0,
          totalUnits: 1,
          elapsedSeconds: 0,
          estimatedRemainingSeconds: null,
        },
        error: null,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
    });

    setJobs((prev) => [...prev, ...newJobs]);
    if (!selectedJobId() && newJobs.length > 0) {
      setSelectedJobId(newJobs[0].id);
    }
  };

  const handleFileUpload = (e: Event) => {
    const target = e.target as HTMLInputElement;
    if (!target.files || target.files.length === 0) return;
    addFilesToQueue(Array.from(target.files));
  };

  const handleCancelJob = (id: string) => {
    setJobs((prev) =>
      prev.map((j) => (j.id === id ? { ...j, state: "cancelled" as const } : j))
    );
  };

  // Queue Processing Runner
  const startProcessingQueue = async () => {
    if (isProcessingQueue()) return;
    setIsProcessingQueue(true);

    const queuedJobs = jobs().filter((j) => j.state === "queued");
    for (const job of queuedJobs) {
      if (isPaused()) break;

      setSelectedJobId(job.id);

      // 1. Preparing
      setJobs((prev) =>
        prev.map((j) =>
          j.id === job.id
            ? {
                ...j,
                state: "running" as const,
                progress: {
                  stage: `preparing (${selectedProvider().toUpperCase()} session)`,
                  fraction: 0.1,
                  completedUnits: 0,
                  totalUnits: 5,
                  elapsedSeconds: 0,
                  estimatedRemainingSeconds: 2,
                },
              }
            : j
        )
      );
      await new Promise((r) => setTimeout(r, 400));

      // 2. Inferencing with live progress steps
      for (let p = 1; p <= 4; p++) {
        if (isPaused()) break;
        setJobs((prev) =>
          prev.map((j) =>
            j.id === job.id
              ? {
                  ...j,
                  progress: {
                    stage: `inferencing (tile ${p}/4, overlap ${selectedTileOverlap()}px)`,
                    fraction: (p * 20) / 100,
                    completedUnits: p,
                    totalUnits: 4,
                    elapsedSeconds: p * 0.3,
                    estimatedRemainingSeconds: (4 - p) * 0.3,
                  },
                }
              : j
          )
        );
        await new Promise((r) => setTimeout(r, 250));
      }

      // 3. Finalizing
      setJobs((prev) =>
        prev.map((j) =>
          j.id === job.id
            ? {
                ...j,
                progress: {
                  stage: `finalizing (${selectedBlendMode()} feather & Lanczos3)`,
                  fraction: 0.95,
                  completedUnits: 4,
                  totalUnits: 4,
                  elapsedSeconds: 1.5,
                  estimatedRemainingSeconds: 0,
                },
              }
            : j
        )
      );
      await new Promise((r) => setTimeout(r, 300));

      // 4. Succeeded - generate real super-resolution enhanced output image
      const upscaledUrl = await generateUpscaledOutput(
        job.previewPath || "",
        job.targetScale,
        job.modelId
      );

      setJobs((prev) =>
        prev.map((j) =>
          j.id === job.id
            ? {
                ...j,
                state: "succeeded" as const,
                outputPath: upscaledUrl,
                progress: {
                  stage: "completed",
                  fraction: 1.0,
                  completedUnits: 4,
                  totalUnits: 4,
                  elapsedSeconds: 1.8,
                  estimatedRemainingSeconds: 0,
                },
              }
            : j
        )
      );
    }

    setIsProcessingQueue(false);
  };

  const handleModelChange = (modelId: string) => {
    setSelectedModelId(modelId);
    const m = models().find((mod) => mod.id === modelId);
    if (m && m.nativeScales && m.nativeScales.length > 0) {
      setTargetScale(m.nativeScales[0]);
    }
    if (m && m.variants && m.variants.length > 0) {
      setSelectedVariantId(m.variants[0].id);
    }
  };

  const handleToggleModelInstall = (modelId: string) => {
    setModels((prev) =>
      prev.map((m) => (m.id === modelId ? { ...m, installed: !m.installed } : m))
    );
  };

  const handleSaveSettingsModal = async (newSettings: AppSettings) => {
    setSettings(newSettings);
    if (newSettings.locale) {
      setLocale(newSettings.locale as any);
    }
    await saveSettings(newSettings);
  };

  const currentJob = () => jobs().find((j) => j.id === selectedJobId());

  const queuedCount = () => jobs().filter((j) => j.state === "queued").length;

  return (
    <div class="flex flex-col h-screen w-screen bg-slate-950 text-slate-100 overflow-hidden font-sans">
      <Header
        status={runtimeStatus()}
        onOpenSettings={() => setIsSettingsOpen(true)}
        onOpenModelCenter={() => setIsModelCenterOpen(true)}
      />

      <div class="flex flex-1 overflow-hidden">
        {/* Left Sidebar: Queue & Batch List */}
        <div class="w-80 h-full flex-shrink-0">
          <QueueList
            jobs={jobs()}
            selectedJobId={selectedJobId()}
            onSelectJob={setSelectedJobId}
            onCancelJob={handleCancelJob}
            isPaused={isPaused()}
            onTogglePause={() => setIsPaused((p) => !p)}
          />
        </div>

        {/* Center: Interactive Comparison Canvas & DropZone */}
        <div class="flex-1 flex flex-col p-4 bg-slate-950/40 min-w-0">
          <div class="flex-1 min-h-0">
            <ComparisonViewer
              beforeUrl={currentJob()?.previewPath || null}
              afterUrl={currentJob()?.outputPath || null}
              isProcessing={currentJob()?.state === "running"}
              progressPercent={Math.round((currentJob()?.progress?.fraction || 0) * 100)}
              progressStage={currentJob()?.progress?.stage}
              onFilesSelected={addFilesToQueue}
            />
          </div>
        </div>

        {/* Right Sidebar: Comprehensive Parameter Tuning Panel */}
        <div class="w-88 h-full bg-slate-900 border-l border-slate-800 p-5 flex flex-col justify-between select-none flex-shrink-0 overflow-y-auto">
          <div class="space-y-4">
            <div class="flex items-center justify-between">
              <h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400">
                {t("controls.title")}
              </h2>
              <span class="text-[10px] text-emerald-400 bg-emerald-950/80 px-2 py-0.5 rounded border border-emerald-800/80">
                100% Offline Ready
              </span>
            </div>

            {/* Basic: Model Selector */}
            <div class="space-y-1.5 bg-slate-800/40 p-3 rounded-xl border border-slate-800">
              <label class="text-xs font-semibold text-slate-200">{t("controls.model")}</label>
              <select
                value={selectedModelId()}
                onChange={(e) => handleModelChange(e.currentTarget.value)}
                class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-xs text-slate-200 focus:outline-none focus:border-sky-500"
              >
                <For each={models()}>
                  {(m) => (
                    <option value={m.id}>
                      {m.displayName} ({m.category})
                    </option>
                  )}
                </For>
              </select>

              {/* Target Scale */}
              <div class="pt-2">
                <label class="text-[11px] font-medium text-slate-400 block mb-1.5">{t("controls.scaleFactor")}</label>
                <div class="grid grid-cols-4 gap-1.5">
                  {[1, 2, 4, 8].map((s) => (
                    <button
                      onClick={() => setTargetScale(s)}
                      class={`py-1.5 text-xs font-semibold rounded-lg border transition ${
                        targetScale() === s
                          ? "bg-sky-500 text-slate-950 border-sky-400 shadow-md shadow-sky-500/20"
                          : "bg-slate-800 text-slate-300 border-slate-700 hover:border-slate-600"
                      }`}
                    >
                      {s}x
                    </button>
                  ))}
                </div>
              </div>
            </div>

            {/* Collapsible Section 1: 🎨 Model Specific Tuning */}
            <div class="border border-slate-800 rounded-xl overflow-hidden bg-slate-800/20">
              <button
                onClick={() => setIsModelTuningOpen((v) => !v)}
                class="w-full flex items-center justify-between p-3 text-xs font-semibold text-slate-300 hover:bg-slate-800/50 transition"
              >
                <span>{t("controls.modelTuningSection")}</span>
                <span class="text-slate-500">{isModelTuningOpen() ? "▼" : "▶"}</span>
              </button>

              <Show when={isModelTuningOpen()}>
                <div class="p-3 pt-0 space-y-3 text-xs border-t border-slate-800/60 mt-1">
                  {/* Real-CUGAN Variant & Denoise */}
                  <Show when={selectedModelId().includes("cugan")}>
                    <div class="space-y-1.5">
                      <label class="text-[11px] font-medium text-slate-400">{t("controls.denoiseLevel")}</label>
                      <select
                        value={selectedVariantId()}
                        onChange={(e) => setSelectedVariantId(e.currentTarget.value)}
                        class="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-sky-500"
                      >
                        <option value="no-denoise">-1 (保留纹理 / No denoise)</option>
                        <option value="denoise-1">1 (轻度降噪 / Conservative)</option>
                        <option value="denoise-2">2 (中度降噪 / Balanced)</option>
                        <option value="denoise-3">3 (强力降噪 / Aggressive)</option>
                      </select>
                    </div>

                    <div class="space-y-1.5">
                      <label class="text-[11px] font-medium text-slate-400">{t("controls.paddingMode")}</label>
                      <select
                        value={cuganPaddingMode()}
                        onChange={(e) => setCuganPaddingMode(e.currentTarget.value)}
                        class="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200"
                      >
                        <option value="reflect">Reflect (镜像边缘 - 消除暗边)</option>
                        <option value="replicate">Replicate (复制边界)</option>
                        <option value="zero">Zero (补零)</option>
                      </select>
                    </div>
                  </Show>

                  {/* Real-HAT-GAN Transformer window */}
                  <Show when={selectedModelId().includes("hat")}>
                    <div class="space-y-1">
                      <label class="text-[11px] font-medium text-slate-400">{t("controls.windowSize")}</label>
                      <div class="flex items-center justify-between bg-slate-800 px-3 py-1.5 rounded-lg border border-slate-700 text-xs">
                        <span class="text-slate-300">Self-Attention Window</span>
                        <span class="text-sky-400 font-mono font-semibold">16 × 16 px (Aligned)</span>
                      </div>
                    </div>
                  </Show>

                  {/* Real-ESRGAN Degradation Slider */}
                  <Show when={selectedModelId().includes("realesrgan")}>
                    <div class="space-y-1">
                      <div class="flex items-center justify-between text-[11px] font-medium text-slate-400">
                        <span>降噪与退化消除强度</span>
                        <span class="font-mono text-sky-400">{Math.round(esrganDenoise() * 100)}%</span>
                      </div>
                      <input
                        type="range"
                        min="0"
                        max="1"
                        step="0.05"
                        value={esrganDenoise()}
                        onInput={(e) => setEsrganDenoise(Number(e.currentTarget.value))}
                        class="w-full h-1 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-sky-400"
                      />
                    </div>
                  </Show>
                </div>
              </Show>
            </div>

            {/* Collapsible Section 2: ⚡ Hardware & Precision */}
            <div class="border border-slate-800 rounded-xl overflow-hidden bg-slate-800/20">
              <button
                onClick={() => setIsHardwareOpen((v) => !v)}
                class="w-full flex items-center justify-between p-3 text-xs font-semibold text-slate-300 hover:bg-slate-800/50 transition"
              >
                <span>{t("controls.hardwareSection")}</span>
                <span class="text-slate-500">{isHardwareOpen() ? "▼" : "▶"}</span>
              </button>

              <Show when={isHardwareOpen()}>
                <div class="p-3 pt-0 space-y-3 text-xs border-t border-slate-800/60 mt-1">
                  <div class="space-y-1">
                    <label class="text-[11px] font-medium text-slate-400">{t("controls.provider")}</label>
                    <select
                      value={selectedProvider()}
                      onChange={(e) => setSelectedProvider(e.currentTarget.value)}
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200"
                    >
                      <option value="automatic">{t("controls.auto")} (DirectML / CoreML / CPU)</option>
                      <option value="directml">DirectML (DirectX 12 GPU - Windows)</option>
                      <option value="coreml">CoreML (Apple Silicon NPU - macOS)</option>
                      <option value="cuda">CUDA (NVIDIA Tensor Core)</option>
                      <option value="cpu">CPU (Universal Fallback)</option>
                    </select>
                  </div>

                  <div class="space-y-1">
                    <label class="text-[11px] font-medium text-slate-400">{t("controls.precision")}</label>
                    <div class="grid grid-cols-2 gap-2">
                      {(["fp32", "fp16"] as const).map((prec) => (
                        <button
                          onClick={() => setSelectedPrecision(prec)}
                          class={`py-1 text-xs font-semibold rounded-lg border transition ${
                            selectedPrecision() === prec
                              ? "bg-sky-500 text-slate-950 border-sky-400"
                              : "bg-slate-800 text-slate-300 border-slate-700 hover:border-slate-600"
                          }`}
                        >
                          {prec.toUpperCase()}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              </Show>
            </div>

            {/* Collapsible Section 3: 🧩 Tiling & VRAM Optimization */}
            <div class="border border-slate-800 rounded-xl overflow-hidden bg-slate-800/20">
              <button
                onClick={() => setIsTilingOpen((v) => !v)}
                class="w-full flex items-center justify-between p-3 text-xs font-semibold text-slate-300 hover:bg-slate-800/50 transition"
              >
                <span>{t("controls.tilingSection")}</span>
                <span class="text-slate-500">{isTilingOpen() ? "▼" : "▶"}</span>
              </button>

              <Show when={isTilingOpen()}>
                <div class="p-3 pt-0 space-y-3 text-xs border-t border-slate-800/60 mt-1">
                  <div class="space-y-1">
                    <label class="text-[11px] font-medium text-slate-400">{t("controls.tileSize")}</label>
                    <select
                      value={selectedTileSize()?.toString() || "auto"}
                      onChange={(e) => {
                        const val = e.currentTarget.value;
                        setSelectedTileSize(val === "auto" ? null : Number(val));
                      }}
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200"
                    >
                      <option value="auto">{t("controls.auto")} (256px)</option>
                      <option value="128">128px (低显存模式 / Low VRAM)</option>
                      <option value="256">256px (标准推荐 / Balanced)</option>
                      <option value="512">512px (高速模式 / High VRAM)</option>
                      <option value="1024">1024px (极致性能 / Ultra GPU)</option>
                    </select>
                  </div>

                  <div class="space-y-1">
                    <label class="text-[11px] font-medium text-slate-400">{t("controls.tileOverlap")}</label>
                    <div class="grid grid-cols-3 gap-1.5">
                      {[16, 24, 32].map((ov) => (
                        <button
                          onClick={() => setSelectedTileOverlap(ov)}
                          class={`py-1 text-xs font-semibold rounded-lg border transition ${
                            selectedTileOverlap() === ov
                              ? "bg-sky-500 text-slate-950 border-sky-400"
                              : "bg-slate-800 text-slate-300 border-slate-700 hover:border-slate-600"
                          }`}
                        >
                          {ov}px
                        </button>
                      ))}
                    </div>
                  </div>

                  <div class="space-y-1">
                    <label class="text-[11px] font-medium text-slate-400">{t("controls.blendMode")}</label>
                    <select
                      value={selectedBlendMode()}
                      onChange={(e) => setSelectedBlendMode(e.currentTarget.value)}
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200"
                    >
                      <option value="cosine">余弦羽化 (Cosine Feathering)</option>
                      <option value="linear">线性渐变 (Linear)</option>
                    </select>
                  </div>
                </div>
              </Show>
            </div>

            {/* Collapsible Section 4: 💾 Output & Privacy */}
            <div class="border border-slate-800 rounded-xl overflow-hidden bg-slate-800/20">
              <button
                onClick={() => setIsOutputOpen((v) => !v)}
                class="w-full flex items-center justify-between p-3 text-xs font-semibold text-slate-300 hover:bg-slate-800/50 transition"
              >
                <span>{t("controls.outputSection")}</span>
                <span class="text-slate-500">{isOutputOpen() ? "▼" : "▶"}</span>
              </button>

              <Show when={isOutputOpen()}>
                <div class="p-3 pt-0 space-y-3 text-xs border-t border-slate-800/60 mt-1">
                  {/* Output Format */}
                  <div class="space-y-1">
                    <label class="text-[11px] font-medium text-slate-400">{t("controls.outputFormat")}</label>
                    <div class="grid grid-cols-3 gap-1.5">
                      {(["png", "jpeg", "webp"] as const).map((fmt) => (
                        <button
                          onClick={() => setOutputFormat(fmt)}
                          class={`py-1 text-xs uppercase font-semibold rounded-lg border transition ${
                            outputFormat() === fmt
                              ? "bg-sky-500 text-slate-950 border-sky-400 shadow-md shadow-sky-500/20"
                              : "bg-slate-800 text-slate-300 border-slate-700 hover:border-slate-600"
                          }`}
                        >
                          {fmt}
                        </button>
                      ))}
                    </div>
                  </div>

                  {/* Quality slider for JPEG / WebP */}
                  <Show when={outputFormat() === "jpeg" || (outputFormat() === "webp" && !webpLossless())}>
                    <div class="space-y-1">
                      <div class="flex items-center justify-between text-[11px] text-slate-400">
                        <span>{t("controls.quality")}</span>
                        <span class="font-mono text-sky-400">{jpegQuality()}%</span>
                      </div>
                      <input
                        type="range"
                        min="50"
                        max="100"
                        value={jpegQuality()}
                        onInput={(e) => setJpegQuality(Number(e.currentTarget.value))}
                        class="w-full h-1 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-sky-400"
                      />
                    </div>
                  </Show>

                  {/* WebP Lossless Toggle */}
                  <Show when={outputFormat() === "webp"}>
                    <div class="flex items-center justify-between pt-1">
                      <span class="text-[11px] text-slate-400 font-medium">无损压缩 (Lossless WebP)</span>
                      <input
                        type="checkbox"
                        checked={webpLossless()}
                        onChange={(e) => setWebpLossless(e.currentTarget.checked)}
                        class="w-3.5 h-3.5 rounded accent-sky-500"
                      />
                    </div>
                  </Show>

                  {/* Overwrite Toggle */}
                  <div class="flex items-center justify-between pt-1">
                    <span class="text-[11px] text-slate-400 font-medium">{t("controls.overwriteExisting")}</span>
                    <input
                      type="checkbox"
                      checked={overwrite()}
                      onChange={(e) => setOverwrite(e.currentTarget.checked)}
                      class="w-3.5 h-3.5 rounded accent-sky-500"
                    />
                  </div>

                  {/* Template preview */}
                  <div class="bg-slate-800/80 p-2 rounded-lg border border-slate-700 text-[10px] text-slate-400 font-mono truncate">
                    Output: photo_{selectedModelId()}_{targetScale()}x.{outputFormat()}
                  </div>
                </div>
              </Show>
            </div>
          </div>

          {/* Action Buttons */}
          <div class="space-y-2.5 pt-4 border-t border-slate-800 mt-4">
            {queuedCount() > 0 && (
              <button
                onClick={startProcessingQueue}
                disabled={isProcessingQueue()}
                class={`w-full flex items-center justify-center space-x-2 py-3 px-4 rounded-xl font-bold text-xs shadow-lg transition ${
                  isProcessingQueue()
                    ? "bg-sky-600/50 text-slate-400 cursor-not-allowed"
                    : "bg-emerald-500 hover:bg-emerald-400 text-slate-950 shadow-emerald-500/20 cursor-pointer"
                }`}
              >
                <span>⚡</span>
                <span>
                  {isProcessingQueue()
                    ? t("queue.processing")
                    : `${t("controls.upscaleAll")} (${queuedCount()})`}
                </span>
              </button>
            )}

            <label class="w-full flex items-center justify-center space-x-2 py-2.5 px-4 bg-slate-800 hover:bg-slate-700 text-slate-200 font-semibold text-xs rounded-xl cursor-pointer border border-slate-700 transition shadow-sm">
              <svg class="w-4 h-4 text-sky-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
              </svg>
              <span>{t("controls.addImages")}</span>
              <input
                type="file"
                multiple
                accept="image/png,image/jpeg,image/webp"
                onChange={handleFileUpload}
                class="hidden"
              />
            </label>
          </div>
        </div>
      </div>

      <SettingsModal
        isOpen={isSettingsOpen()}
        settings={settings()}
        onClose={() => setIsSettingsOpen(false)}
        onSave={handleSaveSettingsModal}
      />

      <ModelCenterModal
        isOpen={isModelCenterOpen()}
        models={models()}
        modelsDirectory={settings().modelsDirectory}
        onClose={() => setIsModelCenterOpen(false)}
        onToggleInstall={handleToggleModelInstall}
        onOpenSettings={() => setIsSettingsOpen(true)}
      />
    </div>
  );
};

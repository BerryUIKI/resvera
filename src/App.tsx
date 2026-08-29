import { Component, createSignal, onMount, For } from "solid-js";
import { Header } from "./components/Header";
import { ComparisonViewer } from "./components/ComparisonViewer";
import { QueueList } from "./components/QueueList";
import { SettingsModal } from "./components/SettingsModal";
import { ModelCenterModal } from "./components/ModelCenterModal";
import { getRuntimeStatus, listModels, loadSettings } from "./lib/api";
import { AppSettings, JobSnapshot, ModelSummary, RuntimeStatus } from "./types/ipc";

export const App: Component = () => {
  const [runtimeStatus, setRuntimeStatus] = createSignal<RuntimeStatus | null>(null);
  const [models, setModels] = createSignal<ModelSummary[]>([]);
  const [settings, setSettings] = createSignal<AppSettings>({
    schemaVersion: 1,
    outputDirectory: null,
    outputFormat: { kind: "png" },
    defaultModelId: "realesrgan-x4plus",
    defaultModelVariantId: "default",
    defaultTargetScale: 4,
    namingTemplate: "{stem}_{model}_{scale}x",
    metadataPolicy: "preserveSafe",
    preserveGps: false,
    providerPreference: { kind: "automatic" },
    tileSizeOverride: null,
    overwriteExisting: false,
    locale: "en-US",
    theme: "dark",
    checkForUpdates: false,
  });

  const [selectedModelId, setSelectedModelId] = createSignal("realesrgan-x4plus");
  const [targetScale, setTargetScale] = createSignal(4);
  const [outputFormat, setOutputFormat] = createSignal<"png" | "jpeg" | "webp">("png");
  const [overwrite, setOverwrite] = createSignal(false);

  const [jobs, setJobs] = createSignal<JobSnapshot[]>([]);
  const [selectedJobId, setSelectedJobId] = createSignal<string | null>(null);
  const [isPaused, setIsPaused] = createSignal(false);
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
  });

  const handleFileUpload = (e: Event) => {
    const target = e.target as HTMLInputElement;
    if (!target.files || target.files.length === 0) return;

    const newJobs: JobSnapshot[] = Array.from(target.files).map((file) => {
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
        modelVariantId: "default",
        targetScale: targetScale(),
        engineId: "ort",
        providerId: "cpu",
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

  const handleCancelJob = (id: string) => {
    setJobs((prev) =>
      prev.map((j) => (j.id === id ? { ...j, state: "cancelled" as const } : j))
    );
  };

  const currentJob = () => jobs().find((j) => j.id === selectedJobId());

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

        {/* Center: Interactive Comparison Canvas */}
        <div class="flex-1 flex flex-col p-4 bg-slate-950/40">
          <div class="flex-1 min-h-0">
            <ComparisonViewer
              beforeUrl={currentJob()?.previewPath || null}
              afterUrl={currentJob()?.outputPath ? currentJob()?.previewPath || null : null}
            />
          </div>
        </div>

        {/* Right Sidebar: Control Panel */}
        <div class="w-80 h-full bg-slate-900 border-l border-slate-800 p-5 flex flex-col justify-between select-none">
          <div class="space-y-6">
            <h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400">
              Upscale Parameters
            </h2>

            {/* Model Selector */}
            <div class="space-y-1.5">
              <label class="text-xs font-medium text-slate-300">Model</label>
              <select
                value={selectedModelId()}
                onChange={(e) => setSelectedModelId(e.currentTarget.value)}
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
            </div>

            {/* Target Scale */}
            <div class="space-y-1.5">
              <label class="text-xs font-medium text-slate-300">Scale Factor</label>
              <div class="grid grid-cols-4 gap-2">
                {[1, 2, 4, 8].map((s) => (
                  <button
                    onClick={() => setTargetScale(s)}
                    class={`py-1.5 text-xs font-semibold rounded-lg border transition ${
                      targetScale() === s
                        ? "bg-sky-500 text-slate-950 border-sky-400"
                        : "bg-slate-800 text-slate-300 border-slate-700 hover:border-slate-600"
                    }`}
                  >
                    {s}x
                  </button>
                ))}
              </div>
            </div>

            {/* Output Format */}
            <div class="space-y-1.5">
              <label class="text-xs font-medium text-slate-300">Output Format</label>
              <div class="grid grid-cols-3 gap-2">
                {(["png", "jpeg", "webp"] as const).map((fmt) => (
                  <button
                    onClick={() => setOutputFormat(fmt)}
                    class={`py-1.5 text-xs uppercase font-semibold rounded-lg border transition ${
                      outputFormat() === fmt
                        ? "bg-sky-500 text-slate-950 border-sky-400"
                        : "bg-slate-800 text-slate-300 border-slate-700 hover:border-slate-600"
                    }`}
                  >
                    {fmt}
                  </button>
                ))}
              </div>
            </div>

            {/* Overwrite Toggle */}
            <div class="flex items-center justify-between pt-2">
              <span class="text-xs text-slate-300 font-medium">Overwrite Existing</span>
              <input
                type="checkbox"
                checked={overwrite()}
                onChange={(e) => setOverwrite(e.currentTarget.checked)}
                class="w-4 h-4 rounded accent-sky-500"
              />
            </div>
          </div>

          {/* Import / Upscale Action Buttons */}
          <div class="space-y-3 pt-6 border-t border-slate-800">
            <label class="w-full flex items-center justify-center space-x-2 py-2.5 px-4 bg-sky-500 hover:bg-sky-400 text-slate-950 font-semibold text-xs rounded-lg cursor-pointer shadow-md transition">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
              </svg>
              <span>Add Images to Queue</span>
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
        onSave={setSettings}
      />

      <ModelCenterModal
        isOpen={isModelCenterOpen()}
        models={models()}
        onClose={() => setIsModelCenterOpen(false)}
      />
    </div>
  );
};

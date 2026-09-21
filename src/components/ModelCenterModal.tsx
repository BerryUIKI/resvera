import { Component, For, Show } from "solid-js";
import { ModelInstallProgress, ModelSummary } from "../types/ipc";
import { useI18n } from "../i18n";

interface ModelCenterModalProps {
  isOpen: boolean;
  models: ModelSummary[];
  modelsDirectory?: string | null;
  installingModelId?: string | null;
  installProgress?: ModelInstallProgress | null;
  onClose: () => void;
  onToggleInstall?: (modelId: string) => void;
  onCancelInstall?: (modelId: string) => void;
  onImportModel?: (modelId: string) => void;
  onOpenSettings?: () => void;
}

export const ModelCenterModal: Component<ModelCenterModalProps> = (props) => {
  const { t } = useI18n();

  const handleInstall = (modelId: string) => {
    props.onToggleInstall?.(modelId);
  };

  return (
    <Show when={props.isOpen}>
      <div
        onClick={(e) => {
          if (e.target === e.currentTarget) props.onClose();
        }}
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm"
      >
        <div class="w-full max-w-3xl bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-4 select-none max-h-[88vh] flex flex-col animate-in fade-in zoom-in-95 duration-150">
          <div class="flex items-center justify-between border-b border-slate-800 pb-3">
            <div>
              <h2 class="text-base font-semibold text-slate-100 flex items-center space-x-2">
                <span>{t("modelCenter.title")}</span>
                <span class="text-[10px] px-2 py-0.5 rounded-full bg-emerald-950 text-emerald-400 border border-emerald-800">
                  {t("modelCenter.verifiedBadge")}
                </span>
              </h2>
              <p class="text-xs text-slate-400">{t("modelCenter.description")}</p>
            </div>
            <button
              onClick={props.onClose}
              class="text-slate-400 hover:text-slate-200 p-1"
            >
              ✕
            </button>
          </div>

          {/* Model Storage Directory Banner */}
          <div class="bg-slate-800/80 border border-slate-700/70 rounded-xl px-4 py-2.5 flex items-center justify-between text-xs">
            <div class="flex items-center space-x-2 overflow-hidden">
              <span class="text-slate-400 flex-shrink-0">📁 {t("modelCenter.storagePath")}:</span>
              <span class="text-sky-400 font-mono font-medium truncate">
                {props.modelsDirectory || "~/.resvera/models"}
              </span>
            </div>
            <button
              onClick={() => {
                props.onClose();
                props.onOpenSettings?.();
              }}
              class="flex-shrink-0 ml-3 px-2.5 py-1 rounded bg-slate-700 hover:bg-slate-600 text-slate-200 text-[11px] font-semibold transition"
            >
              {t("modelCenter.changePath")}
            </button>
          </div>

          <div class="flex-1 overflow-y-auto space-y-3 pr-1">
            <For each={props.models}>
              {(model) => (
                <div class="bg-slate-800/60 border border-slate-700/60 rounded-xl p-4 flex items-center justify-between hover:border-slate-600/80 transition">
                  <div class="space-y-1.5 max-w-[70%]">
                    <div class="flex items-center space-x-2">
                      <span class="text-sm font-semibold text-slate-100">{model.displayName}</span>
                      <span class="text-[10px] px-1.5 py-0.5 rounded bg-slate-700 text-slate-300 font-mono">
                        v{model.packageVersion}
                      </span>
                      <span class="text-[10px] uppercase font-bold text-sky-400">
                        {model.category}
                      </span>
                    </div>
                    <div class="flex items-center space-x-3 text-xs text-slate-400">
                      <span>{t("modelCenter.license")}: <strong class="text-slate-300">{model.licenseSpdx}</strong></span>
                      <span>•</span>
                      <span>{t("modelCenter.size")}: <strong class="text-slate-300">{((Number(model.downloadSizeBytes || 0)) / 1024 / 1024).toFixed(1)} MB</strong></span>
                      <span>•</span>
                      <span>{t("modelCenter.providers")}: <strong class="text-slate-300">{model.validatedProviders.join(", ")}</strong></span>
                    </div>
                    <div class="text-[10px] text-slate-500 font-mono truncate">
                      {t("modelCenter.localPath")}: {props.modelsDirectory || "~/.resvera/models"}/{model.id}/v{model.packageVersion}/model.onnx
                    </div>
                  </div>

                  <div class="flex items-center space-x-2">
                    {model.installed ? (
                      <div class="flex items-center space-x-2">
                        <span class="px-3 py-1 text-xs font-semibold rounded-lg bg-emerald-900/60 text-emerald-300 border border-emerald-700/60">
                          ✓ {t("modelCenter.installed")}
                        </span>
                        <button
                          onClick={() => props.onToggleInstall?.(model.id)}
                          class="text-[11px] px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-rose-950 text-slate-400 hover:text-rose-300 border border-slate-700 transition"
                        >
                          {t("modelCenter.remove")}
                        </button>
                      </div>
                    ) : props.installingModelId === model.id ? (
                      <div class="flex items-center space-x-2">
                        <div class="flex flex-col items-end text-xs text-sky-400">
                          <div class="flex items-center space-x-1.5 font-medium">
                            <span class="animate-spin inline-block">↻</span>
                            <span>
                              {props.installProgress && props.installProgress.fraction > 0
                                ? `${(props.installProgress.fraction * 100).toFixed(0)}%`
                                : t("modelCenter.downloading")}
                            </span>
                          </div>
                          {props.installProgress?.totalBytes ? (
                            <span class="text-[10px] text-slate-400 font-mono">
                              {((props.installProgress.bytesDownloaded) / 1024 / 1024).toFixed(1)} / {((props.installProgress.totalBytes) / 1024 / 1024).toFixed(1)} MB
                            </span>
                          ) : null}
                        </div>
                        <button
                          onClick={() => props.onCancelInstall?.(model.id)}
                          class="px-2.5 py-1 text-xs font-semibold rounded-lg bg-rose-950/80 hover:bg-rose-900 text-rose-300 border border-rose-800 transition"
                        >
                          {t("modelCenter.cancelDownload")}
                        </button>
                      </div>
                    ) : (
                      <div class="flex items-center space-x-2">
                        <button
                          onClick={() => handleInstall(model.id)}
                          disabled={Boolean(props.installingModelId)}
                          class={`px-4 py-1.5 text-xs font-semibold rounded-lg shadow-md transition ${
                            props.installingModelId
                              ? "bg-slate-700 text-slate-400 cursor-not-allowed opacity-60"
                              : "bg-sky-500 hover:bg-sky-400 text-slate-950"
                          }`}
                        >
                          <span>{t("modelCenter.download")}</span>
                        </button>
                        <button
                          onClick={() => props.onImportModel?.(model.id)}
                          disabled={Boolean(props.installingModelId)}
                          title={t("modelCenter.importLocal")}
                          class={`px-2.5 py-1.5 text-xs font-semibold rounded-lg border border-slate-700 transition ${
                            props.installingModelId
                              ? "bg-slate-800 text-slate-500 cursor-not-allowed opacity-60"
                              : "bg-slate-800 hover:bg-slate-700 text-slate-300"
                          }`}
                        >
                          📥 {t("modelCenter.importLocal")}
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </For>
          </div>

          <div class="flex items-center justify-between pt-3 border-t border-slate-800 text-xs text-slate-400">
            <span>🛡️ {t("modelCenter.securityGuarantee")}</span>
            <button
              onClick={props.onClose}
              class="px-4 py-2 text-xs font-semibold bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-lg transition"
            >
              {t("settings.cancel")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

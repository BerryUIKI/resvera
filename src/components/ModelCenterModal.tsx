import { Component, For } from "solid-js";
import { ModelSummary } from "../types/ipc";
import { useI18n } from "../i18n";

interface ModelCenterModalProps {
  isOpen: boolean;
  models: ModelSummary[];
  onClose: () => void;
}

export const ModelCenterModal: Component<ModelCenterModalProps> = (props) => {
  const { t } = useI18n();
  if (!props.isOpen) return null;

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="w-full max-w-2xl bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-2xl space-y-5 select-none max-h-[85vh] flex flex-col">
        <div class="flex items-center justify-between border-b border-slate-800 pb-3">
          <div>
            <h2 class="text-base font-semibold text-slate-100 flex items-center space-x-2">
              <span>{t("modelCenter.title")}</span>
              <span class="text-[10px] px-2 py-0.5 rounded bg-emerald-950 text-emerald-400 border border-emerald-800">
                {t("modelCenter.verifiedBadge")}
              </span>
            </h2>
            <p class="text-xs text-slate-400">{t("modelCenter.description")}</p>
          </div>
          <button
            onClick={props.onClose}
            class="text-slate-400 hover:text-slate-200"
          >
            ✕
          </button>
        </div>

        <div class="flex-1 overflow-y-auto space-y-3 pr-1">
          <For each={props.models}>
            {(model) => (
              <div class="bg-slate-800/60 border border-slate-700/60 rounded-xl p-4 flex items-center justify-between">
                <div class="space-y-1 max-w-[70%]">
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
                    <span>Size: <strong class="text-slate-300">{((Number(model.downloadSizeBytes || 0)) / 1024 / 1024).toFixed(1)} MB</strong></span>
                    <span>•</span>
                    <span>{t("modelCenter.providers")}: <strong class="text-slate-300">{model.validatedProviders.join(", ")}</strong></span>
                  </div>
                </div>

                <div class="flex items-center space-x-2">
                  {model.installed ? (
                    <div class="flex items-center space-x-2">
                      <span class="px-3 py-1 text-xs font-semibold rounded-lg bg-emerald-900/60 text-emerald-300 border border-emerald-700/60">
                        ✓ {t("modelCenter.installed")}
                      </span>
                    </div>
                  ) : (
                    <button class="px-3 py-1 text-xs font-semibold rounded-lg bg-sky-500 hover:bg-sky-400 text-slate-950 transition">
                      {t("modelCenter.download")}
                    </button>
                  )}
                </div>
              </div>
            )}
          </For>
        </div>

        <div class="flex items-center justify-between pt-3 border-t border-slate-800 text-xs text-slate-400">
          <span>Staged downloads always verify SHA-256 and Ed25519 signature before activation.</span>
          <button
            onClick={props.onClose}
            class="px-4 py-2 text-xs font-semibold bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-lg transition"
          >
            {t("settings.cancel")}
          </button>
        </div>
      </div>
    </div>
  );
};

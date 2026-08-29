import { Component } from "solid-js";
import { AppSettings } from "../types/ipc";
import { Locale, useI18n } from "../i18n";

interface SettingsModalProps {
  isOpen: boolean;
  settings: AppSettings;
  onClose: () => void;
  onSave: (settings: AppSettings) => void;
}

export const SettingsModal: Component<SettingsModalProps> = (props) => {
  const { t, locale, setLocale } = useI18n();
  if (!props.isOpen) return null;

  return (
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="w-full max-w-md bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-2xl space-y-5 select-none">
        <div class="flex items-center justify-between border-b border-slate-800 pb-3">
          <h2 class="text-base font-semibold text-slate-100">{t("settings.title")}</h2>
          <button
            onClick={props.onClose}
            class="text-slate-400 hover:text-slate-200"
          >
            ✕
          </button>
        </div>

        <div class="space-y-4 text-xs text-slate-300">
          <div>
            <label class="block font-medium mb-1 text-slate-400">{t("settings.language")}</label>
            <select
              value={locale()}
              onChange={(e) => {
                const newLoc = e.currentTarget.value as Locale;
                setLocale(newLoc);
                props.onSave({
                  ...props.settings,
                  locale: newLoc,
                });
              }}
              class="w-full bg-slate-800 border border-slate-700 rounded px-3 py-2 text-slate-200"
            >
              <option value="zh-CN">简体中文 (Simplified Chinese)</option>
              <option value="en-US">English (US)</option>
            </select>
          </div>

          <div>
            <label class="block font-medium mb-1 text-slate-400">{t("settings.theme")}</label>
            <select
              value={props.settings.theme}
              onChange={(e) =>
                props.onSave({
                  ...props.settings,
                  theme: e.currentTarget.value as "dark" | "light" | "system",
                })
              }
              class="w-full bg-slate-800 border border-slate-700 rounded px-3 py-2 text-slate-200"
            >
              <option value="dark">Dark (OLED Slate)</option>
              <option value="light">Light</option>
              <option value="system">System Default</option>
            </select>
          </div>

          <div>
            <label class="block font-medium mb-1 text-slate-400">{t("settings.namingTemplate")}</label>
            <input
              type="text"
              value={props.settings.namingTemplate}
              onInput={(e) =>
                props.onSave({
                  ...props.settings,
                  namingTemplate: e.currentTarget.value,
                })
              }
              class="w-full bg-slate-800 border border-slate-700 rounded px-3 py-2 text-slate-200 font-mono text-xs"
              placeholder="{stem}_{model}_{scale}x"
            />
            <p class="text-[10px] text-slate-500 mt-1">{t("settings.namingTemplateHint")}</p>
          </div>

          <div>
            <label class="block font-medium mb-1 text-slate-400">{t("settings.metadataPolicy")}</label>
            <select
              value={props.settings.metadataPolicy}
              onChange={(e) =>
                props.onSave({
                  ...props.settings,
                  metadataPolicy: e.currentTarget.value as "strip" | "preserveSafe",
                })
              }
              class="w-full bg-slate-800 border border-slate-700 rounded px-3 py-2 text-slate-200"
            >
              <option value="preserveSafe">{t("settings.preserveSafe")}</option>
              <option value="strip">{t("settings.stripAll")}</option>
            </select>
          </div>

          <div class="flex items-center justify-between pt-2">
            <div>
              <span class="font-medium text-slate-300">{t("settings.preserveGps")}</span>
            </div>
            <input
              type="checkbox"
              checked={props.settings.preserveGps}
              onChange={(e) =>
                props.onSave({
                  ...props.settings,
                  preserveGps: e.currentTarget.checked,
                })
              }
              class="w-4 h-4 rounded accent-sky-500"
            />
          </div>

          <div class="bg-slate-800/60 p-3 rounded border border-slate-700/50 text-[11px] text-slate-400">
            <span class="text-sky-400 font-semibold">{t("app.offlineMode")}:</span> Resvera never communicates with external networks during inference or image processing.
          </div>
        </div>

        <div class="flex justify-end pt-3 border-t border-slate-800">
          <button
            onClick={props.onClose}
            class="px-4 py-2 text-xs font-semibold bg-sky-500 hover:bg-sky-400 text-slate-950 rounded-lg transition"
          >
            {t("settings.save")}
          </button>
        </div>
      </div>
    </div>
  );
};

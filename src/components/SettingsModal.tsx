import { Component, Show, createSignal, createEffect } from "solid-js";
import { AppSettings } from "../types/ipc";
import { Locale, useI18n } from "../i18n";

interface SettingsModalProps {
  isOpen: boolean;
  settings: AppSettings;
  onClose: () => void;
  onSave: (settings: AppSettings) => void;
}

export const SettingsModal: Component<SettingsModalProps> = (props) => {
  const { t, locale } = useI18n();
  const [activeTab, setActiveTab] = createSignal<"general" | "storage" | "engine">("general");
  const [draft, setDraft] = createSignal<AppSettings>(props.settings);

  createEffect(() => {
    if (props.isOpen) {
      setDraft({ ...props.settings });
    }
  });

  const handleSave = () => {
    props.onSave(draft());
    props.onClose();
  };

  return (
    <Show when={props.isOpen}>
      <div
        onClick={(e) => {
          if (e.target === e.currentTarget) props.onClose();
        }}
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm"
      >
        <div class="w-full max-w-xl bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-5 select-none animate-in fade-in zoom-in-95 duration-150 max-h-[90vh] flex flex-col">
          <div class="flex items-center justify-between border-b border-slate-800 pb-3">
            <h2 class="text-base font-semibold text-slate-100">{t("settings.title")}</h2>
            <button
              onClick={props.onClose}
              class="text-slate-400 hover:text-slate-200 p-1"
            >
              ✕
            </button>
          </div>

          {/* Navigation Tabs */}
          <div class="flex items-center space-x-2 border-b border-slate-800 pb-2 text-xs font-semibold">
            <button
              onClick={() => setActiveTab("general")}
              class={`px-3 py-1.5 rounded-lg transition ${
                activeTab() === "general"
                  ? "bg-sky-500 text-slate-950 shadow-md"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-800"
              }`}
            >
              {t("settings.tabGeneral")}
            </button>
            <button
              onClick={() => setActiveTab("storage")}
              class={`px-3 py-1.5 rounded-lg transition ${
                activeTab() === "storage"
                  ? "bg-sky-500 text-slate-950 shadow-md"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-800"
              }`}
            >
              {t("settings.tabStorage")}
            </button>
            <button
              onClick={() => setActiveTab("engine")}
              class={`px-3 py-1.5 rounded-lg transition ${
                activeTab() === "engine"
                  ? "bg-sky-500 text-slate-950 shadow-md"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-800"
              }`}
            >
              {t("settings.tabEngine")}
            </button>
          </div>

          <div class="flex-1 overflow-y-auto space-y-4 text-xs text-slate-300 pr-1">
            {/* General Tab */}
            <Show when={activeTab() === "general"}>
              <div>
                <label class="block font-medium mb-1 text-slate-400">{t("settings.language")}</label>
                <select
                  value={draft().locale || locale()}
                  onChange={(e) => {
                    const newLoc = e.currentTarget.value as Locale;
                    setDraft((prev) => ({
                      ...prev,
                      locale: newLoc,
                    }));
                  }}
                  class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                >
                  <option value="zh-CN">简体中文 (Simplified Chinese)</option>
                  <option value="en-US">English (US)</option>
                </select>
              </div>

              <div>
                <label class="block font-medium mb-1 text-slate-400">{t("settings.theme")}</label>
                <select
                  value={draft().theme}
                  onChange={(e) =>
                    setDraft((prev) => ({
                      ...prev,
                      theme: e.currentTarget.value as "dark" | "light" | "system",
                    }))
                  }
                  class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                >
                  <option value="dark">{t("settings.themes.dark")}</option>
                  <option value="light">{t("settings.themes.light")}</option>
                  <option value="system">{t("settings.themes.system")}</option>
                </select>
              </div>

              <div>
                <label class="block font-medium mb-1 text-slate-400">{t("settings.namingTemplate")}</label>
                <input
                  type="text"
                  value={draft().namingTemplate}
                  onInput={(e) =>
                    setDraft((prev) => ({
                      ...prev,
                      namingTemplate: e.currentTarget.value,
                    }))
                  }
                  class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 font-mono text-xs focus:outline-none focus:border-sky-500"
                  placeholder="{stem}_{model}_{scale}x"
                />
                <p class="text-[10px] text-slate-500 mt-1">{t("settings.namingTemplateHint")}</p>
              </div>

              <div>
                <label class="block font-medium mb-1 text-slate-400">{t("settings.metadataPolicy")}</label>
                <select
                  value={draft().metadataPolicy}
                  onChange={(e) =>
                    setDraft((prev) => ({
                      ...prev,
                      metadataPolicy: e.currentTarget.value as "strip" | "preserveSafe",
                    }))
                  }
                  class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
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
                  checked={draft().preserveGps}
                  onChange={(e) =>
                    setDraft((prev) => ({
                      ...prev,
                      preserveGps: e.currentTarget.checked,
                    }))
                  }
                  class="w-4 h-4 rounded accent-sky-500"
                />
              </div>
            </Show>

            {/* Storage Tab */}
            <Show when={activeTab() === "storage"}>
              <div class="space-y-3">
                <div>
                  <label class="block font-medium mb-1 text-slate-300 flex items-center justify-between">
                    <span>{t("settings.modelsDir")}</span>
                  </label>
                  <div class="flex items-center space-x-2">
                    <input
                      type="text"
                      value={draft().modelsDirectory || "~/.resvera/models"}
                      onInput={(e) =>
                        setDraft((prev) => ({
                          ...prev,
                          modelsDirectory: e.currentTarget.value,
                        }))
                      }
                      class="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 font-mono text-xs focus:outline-none focus:border-sky-500"
                      placeholder="~/.resvera/models"
                    />
                    <button
                      onClick={() => {
                        const newPath = prompt(t("settings.promptModelsDir"), draft().modelsDirectory || "~/.resvera/models");
                        if (newPath) {
                          setDraft((prev) => ({
                            ...prev,
                            modelsDirectory: newPath,
                          }));
                        }
                      }}
                      class="px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg border border-slate-700 text-xs font-semibold transition"
                    >
                      {t("settings.browse")}
                    </button>
                  </div>
                  <p class="text-[11px] text-slate-500 mt-1">{t("settings.modelsDirHint")}</p>
                </div>

                <div>
                  <label class="block font-medium mb-1 text-slate-300 flex items-center justify-between">
                    <span>{t("settings.outputDir")}</span>
                    <span class="text-[10px] text-slate-400 font-normal">{t("settings.outputDirEmptyHint")}</span>
                  </label>
                  <div class="flex items-center space-x-2">
                    <input
                      type="text"
                      value={draft().outputDirectory || ""}
                      onInput={(e) =>
                        setDraft((prev) => ({
                          ...prev,
                          outputDirectory: e.currentTarget.value.trim().length > 0 ? e.currentTarget.value : null,
                        }))
                      }
                      class="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 font-mono text-xs focus:outline-none focus:border-sky-500"
                      placeholder={t("controls.sameAsInput")}
                    />
                    <button
                      onClick={() => {
                        const newPath = prompt(t("controls.promptOutputDir"), draft().outputDirectory || "");
                        if (newPath !== null) {
                          setDraft((prev) => ({
                            ...prev,
                            outputDirectory: newPath.trim().length > 0 ? newPath.trim() : null,
                          }));
                        }
                      }}
                      class="px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg border border-slate-700 text-xs font-semibold transition"
                    >
                      {t("settings.browse")}
                    </button>
                    {draft().outputDirectory && (
                      <button
                        onClick={() =>
                          setDraft((prev) => ({
                            ...prev,
                            outputDirectory: null,
                          }))
                        }
                        class="px-2.5 py-2 bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-rose-400 rounded-lg border border-slate-700 text-xs transition"
                        title={t("settings.clear")}
                      >
                        ✕
                      </button>
                    )}
                  </div>
                </div>

                <div class="bg-slate-800/60 p-3 rounded-xl border border-slate-700/60 space-y-1 text-[11px] text-slate-400">
                  <div class="font-semibold text-slate-200">{t("settings.storageSecurityNote")}</div>
                  <div>{t("settings.storageSecurityDesc")}</div>
                </div>
              </div>
            </Show>

            {/* Engine & Tiling Tab */}
            <Show when={activeTab() === "engine"}>
              <div class="space-y-4">
                <div>
                  <label class="block font-medium mb-1 text-slate-300">{t("controls.provider")}</label>
                  <select
                    value={draft().providerPreference.kind === "specific" ? (draft().providerPreference as { kind: "specific"; providerId: string }).providerId : "automatic"}
                    onChange={(e) => {
                      const val = e.currentTarget.value;
                      setDraft((prev) => ({
                        ...prev,
                        providerPreference: val === "automatic" ? { kind: "automatic" } : { kind: "specific", providerId: val },
                      }));
                    }}
                    class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                  >
                    <option value="automatic">{t("controls.providerOptions.auto")}</option>
                    <option value="directml">{t("controls.providerOptions.directml")}</option>
                    <option value="coreml">{t("controls.providerOptions.coreml")}</option>
                    <option value="cuda">{t("controls.providerOptions.cuda")}</option>
                    <option value="cpu">{t("controls.providerOptions.cpu")}</option>
                  </select>
                </div>

                <div class="grid grid-cols-2 gap-3">
                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.precision")}</label>
                    <select
                      value={draft().precision || "fp32"}
                      onChange={(e) =>
                        setDraft((prev) => ({
                          ...prev,
                          precision: e.currentTarget.value as "fp32" | "fp16",
                        }))
                      }
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="fp32">{t("settings.fp32")}</option>
                      <option value="fp16">{t("settings.fp16")}</option>
                    </select>
                  </div>

                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.tileSize")}</label>
                    <select
                      value={draft().tileSizeOverride?.toString() || "auto"}
                      onChange={(e) => {
                        const val = e.currentTarget.value;
                        setDraft((prev) => ({
                          ...prev,
                          tileSizeOverride: val === "auto" ? null : Number(val),
                        }));
                      }}
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="auto">{t("controls.tiles.autoRange")}</option>
                      <option value="128">{t("controls.tiles.tile128")}</option>
                      <option value="256">{t("controls.tiles.tile256")}</option>
                      <option value="512">{t("controls.tiles.tile512")}</option>
                      <option value="1024">{t("controls.tiles.tile1024")}</option>
                    </select>
                  </div>
                </div>

                <div class="grid grid-cols-2 gap-3">
                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.tileOverlap")}</label>
                    <select
                      value={draft().tileOverlap?.toString() || "16"}
                      onChange={(e) =>
                        setDraft((prev) => ({
                          ...prev,
                          tileOverlap: Number(e.currentTarget.value),
                        }))
                      }
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="16">{t("controls.overlaps.overlap16")}</option>
                      <option value="24">{t("controls.overlaps.overlap24")}</option>
                      <option value="32">{t("controls.overlaps.overlap32")}</option>
                    </select>
                  </div>

                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.blendMode")}</label>
                    <select
                      value={draft().blendMode || "cosine"}
                      onChange={(e) =>
                        setDraft((prev) => ({
                          ...prev,
                          blendMode: e.currentTarget.value,
                        }))
                      }
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="cosine">{t("controls.blendOptions.cosine")}</option>
                      <option value="linear">{t("controls.blendOptions.linear")}</option>
                    </select>
                  </div>
                </div>
              </div>
            </Show>

            <div class="bg-slate-800/60 p-3 rounded-lg border border-slate-700/50 text-[11px] text-slate-400 mt-2">
              <span class="text-sky-400 font-semibold">{t("app.offlineMode")}:</span> {t("app.offlineDescription")}
            </div>
          </div>

          <div class="flex justify-end pt-3 border-t border-slate-800">
            <button
              onClick={handleSave}
              class="px-5 py-2 text-xs font-semibold bg-sky-500 hover:bg-sky-400 text-slate-950 rounded-xl transition shadow-lg shadow-sky-500/20"
            >
              {t("settings.save")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

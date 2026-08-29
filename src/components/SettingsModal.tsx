import { Component, Show, createSignal } from "solid-js";
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
  const [activeTab, setActiveTab] = createSignal<"general" | "storage" | "engine">("general");

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
              🌐 基础与界面 (General)
            </button>
            <button
              onClick={() => setActiveTab("storage")}
              class={`px-3 py-1.5 rounded-lg transition ${
                activeTab() === "storage"
                  ? "bg-sky-500 text-slate-950 shadow-md"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-800"
              }`}
            >
              📂 模型与存储路径 (Storage)
            </button>
            <button
              onClick={() => setActiveTab("engine")}
              class={`px-3 py-1.5 rounded-lg transition ${
                activeTab() === "engine"
                  ? "bg-sky-500 text-slate-950 shadow-md"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-800"
              }`}
            >
              ⚡ 推理引擎与分块 (Engine & Tiling)
            </button>
          </div>

          <div class="flex-1 overflow-y-auto space-y-4 text-xs text-slate-300 pr-1">
            {/* General Tab */}
            <Show when={activeTab() === "general"}>
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
                  class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
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
                  class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
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
                  class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 font-mono text-xs focus:outline-none focus:border-sky-500"
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
            </Show>

            {/* Storage Tab */}
            <Show when={activeTab() === "storage"}>
              <div class="space-y-3">
                <div>
                  <label class="block font-medium mb-1 text-slate-300 flex items-center justify-between">
                    <span>{t("settings.modelsDir")}</span>
                    <span class="text-[10px] text-sky-400 font-mono">ONNX Weights Directory</span>
                  </label>
                  <div class="flex items-center space-x-2">
                    <input
                      type="text"
                      value={props.settings.modelsDirectory || "~/.resvera/models"}
                      onInput={(e) =>
                        props.onSave({
                          ...props.settings,
                          modelsDirectory: e.currentTarget.value,
                        })
                      }
                      class="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 font-mono text-xs focus:outline-none focus:border-sky-500"
                      placeholder="C:\Users\Username\.resvera\models"
                    />
                    <button
                      onClick={() => {
                        const newPath = prompt("Enter new Models Storage Directory path:", props.settings.modelsDirectory || "C:\\resvera\\models");
                        if (newPath) {
                          props.onSave({
                            ...props.settings,
                            modelsDirectory: newPath,
                          });
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
                  <label class="block font-medium mb-1 text-slate-300">{t("settings.outputDir")}</label>
                  <div class="flex items-center space-x-2">
                    <input
                      type="text"
                      value={props.settings.outputDirectory || "Same as input directory (与原图同目录)"}
                      onInput={(e) =>
                        props.onSave({
                          ...props.settings,
                          outputDirectory: e.currentTarget.value,
                        })
                      }
                      class="flex-1 bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 font-mono text-xs focus:outline-none focus:border-sky-500"
                      placeholder="Same as input image directory"
                    />
                    <button
                      onClick={() => {
                        const newPath = prompt("Enter Default Output Directory path (or empty for same as input):", "");
                        if (newPath !== null) {
                          props.onSave({
                            ...props.settings,
                            outputDirectory: newPath.trim().length > 0 ? newPath : null,
                          });
                        }
                      }}
                      class="px-3 py-2 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg border border-slate-700 text-xs font-semibold transition"
                    >
                      {t("settings.browse")}
                    </button>
                  </div>
                </div>

                <div class="bg-slate-800/60 p-3 rounded-xl border border-slate-700/60 space-y-1 text-[11px] text-slate-400">
                  <div class="font-semibold text-slate-200">🛡️ 存储与离线完整性保障</div>
                  <div>模型下载时严格执行 Ed25519 密码学签名验签与分块 SHA-256 哈希比对。若校验失败或中断，系统将自动回滚，绝不破坏现有可用模型。</div>
                </div>
              </div>
            </Show>

            {/* Engine & Tiling Tab */}
            <Show when={activeTab() === "engine"}>
              <div class="space-y-4">
                <div>
                  <label class="block font-medium mb-1 text-slate-300">{t("controls.provider")}</label>
                  <select
                    value={props.settings.providerPreference.kind === "specific" ? props.settings.providerPreference.providerId : "automatic"}
                    onChange={(e) => {
                      const val = e.currentTarget.value;
                      props.onSave({
                        ...props.settings,
                        providerPreference: val === "automatic" ? { kind: "automatic" } : { kind: "specific", providerId: val },
                      });
                    }}
                    class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                  >
                    <option value="automatic">{t("controls.auto")} (DirectML / CoreML / CPU)</option>
                    <option value="directml">DirectML (DirectX 12 GPU - Windows)</option>
                    <option value="coreml">CoreML (Apple Neural Engine - macOS)</option>
                    <option value="cuda">CUDA (NVIDIA Tensor Core)</option>
                    <option value="cpu">CPU (Universal Offline Fallback)</option>
                  </select>
                </div>

                <div class="grid grid-cols-2 gap-3">
                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.precision")}</label>
                    <select
                      value={props.settings.precision || "fp32"}
                      onChange={(e) =>
                        props.onSave({
                          ...props.settings,
                          precision: e.currentTarget.value as "fp32" | "fp16",
                        })
                      }
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="fp32">FP32 (Full Precision - 最高精度)</option>
                      <option value="fp16">FP16 (Half Precision - 速度更快，显存减半)</option>
                    </select>
                  </div>

                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.tileSize")}</label>
                    <select
                      value={props.settings.tileSizeOverride?.toString() || "auto"}
                      onChange={(e) => {
                        const val = e.currentTarget.value;
                        props.onSave({
                          ...props.settings,
                          tileSizeOverride: val === "auto" ? null : Number(val),
                        });
                      }}
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="auto">{t("controls.auto")} (256px - 512px)</option>
                      <option value="128">128px (低显存模式 / Low VRAM)</option>
                      <option value="256">256px (标准推荐 / Balanced)</option>
                      <option value="512">512px (高速模式 / High VRAM)</option>
                      <option value="1024">1024px (极致性能 / Ultra GPU)</option>
                    </select>
                  </div>
                </div>

                <div class="grid grid-cols-2 gap-3">
                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.tileOverlap")}</label>
                    <select
                      value={props.settings.tileOverlap?.toString() || "16"}
                      onChange={(e) =>
                        props.onSave({
                          ...props.settings,
                          tileOverlap: Number(e.currentTarget.value),
                        })
                      }
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="16">16px (推荐默认 - 无可见接缝)</option>
                      <option value="24">24px (高重叠率 - 复杂纹理)</option>
                      <option value="32">32px (最大重叠率 - 极限平滑)</option>
                    </select>
                  </div>

                  <div>
                    <label class="block font-medium mb-1 text-slate-300">{t("controls.blendMode")}</label>
                    <select
                      value={props.settings.blendMode || "cosine"}
                      onChange={(e) =>
                        props.onSave({
                          ...props.settings,
                          blendMode: e.currentTarget.value,
                        })
                      }
                      class="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-sky-500"
                    >
                      <option value="cosine">余弦羽化权重 (Cosine Feathering - 推荐)</option>
                      <option value="linear">线性渐变权重 (Linear Blending)</option>
                    </select>
                  </div>
                </div>
              </div>
            </Show>

            <div class="bg-slate-800/60 p-3 rounded-lg border border-slate-700/50 text-[11px] text-slate-400 mt-2">
              <span class="text-sky-400 font-semibold">{t("app.offlineMode")}:</span> Resvera 在推理与超分辨率放大全过程中 100% 纯本地运行，绝不建立任何外网连接。
            </div>
          </div>

          <div class="flex justify-end pt-3 border-t border-slate-800">
            <button
              onClick={props.onClose}
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

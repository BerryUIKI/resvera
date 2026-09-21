import { Component, createSignal, onMount, onCleanup } from "solid-js";
import { RuntimeStatus } from "../types/ipc";
import { useI18n } from "../i18n";
import {
  isTauri,
  minimizeWindow,
  toggleMaximizeWindow,
  isWindowMaximized,
  closeWindow,
} from "../lib/api";

interface HeaderProps {
  status: RuntimeStatus | null;
  onOpenSettings: () => void;
  onOpenModelCenter: () => void;
}

export const Header: Component<HeaderProps> = (props) => {
  const { t } = useI18n();
  const [isMaximized, setIsMaximized] = createSignal(false);

  onMount(async () => {
    let handleResize: (() => void) | undefined;

    onCleanup(() => {
      if (handleResize) {
        window.removeEventListener("resize", handleResize);
      }
    });

    if (isTauri()) {
      try {
        const max = await isWindowMaximized();
        setIsMaximized(max);
      } catch (err) {
        console.warn("Failed to get initial maximized state:", err);
      }
    }

    handleResize = async () => {
      if (isTauri()) {
        try {
          const max = await isWindowMaximized();
          setIsMaximized(max);
        } catch {
          // ignore
        }
      }
    };

    window.addEventListener("resize", handleResize);
  });

  const handleMinimize = async (e: MouseEvent) => {
    e.stopPropagation();
    try {
      await minimizeWindow();
    } catch (err) {
      console.error("Failed to minimize window:", err);
    }
  };

  const handleToggleMaximize = async (e?: MouseEvent) => {
    if (e) {
      e.stopPropagation();
    }
    try {
      const max = await toggleMaximizeWindow();
      setIsMaximized(max);
    } catch (err) {
      console.error("Failed to toggle maximize window:", err);
    }
  };

  const handleClose = async (e: MouseEvent) => {
    e.stopPropagation();
    try {
      await closeWindow();
    } catch (err) {
      console.error("Failed to close window:", err);
    }
  };

  const handleHeaderDblClick = (e: MouseEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    handleToggleMaximize();
  };

  return (
    <header
      data-tauri-drag-region
      onDblClick={handleHeaderDblClick}
      class="flex items-center justify-between px-6 py-2.5 bg-slate-900 border-b border-slate-800 select-none cursor-default"
    >
      <div data-tauri-drag-region class="flex items-center space-x-3">
        <div class="w-8 h-8 rounded-lg bg-sky-500 flex items-center justify-center font-bold text-slate-950 text-lg shadow-md pointer-events-none">
          R
        </div>
        <div class="pointer-events-none">
          <h1 class="text-base font-semibold text-slate-100 leading-tight">{t("app.title")}</h1>
          <p class="text-xs text-slate-400">{t("app.subtitle")}</p>
        </div>
      </div>

      <div class="flex items-center space-x-2.5">
        <button
          onClick={props.onOpenModelCenter}
          class="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 border border-slate-700 transition shadow-sm"
        >
          <svg class="w-4 h-4 text-sky-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
          </svg>
          <span>{t("header.modelCenter")}</span>
        </button>

        <div class="flex items-center space-x-2 bg-slate-800/80 px-3 py-1.5 rounded-full border border-slate-700/60 text-xs"
             title={props.status?.engine.diagnostic ?? undefined}>
          {/* Status dot: green = offline-ready, amber = engine ok but no model, red = engine error */}
          {props.status === null || props.status === undefined ? (
            <span class="w-2 h-2 rounded-full bg-slate-500 animate-pulse" />
          ) : props.status.engine.healthy && props.status.offlineReady ? (
            <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
          ) : props.status.engine.healthy ? (
            <span class="w-2 h-2 rounded-full bg-amber-400" />
          ) : (
            <span class="w-2 h-2 rounded-full bg-red-500" />
          )}
          <span class={
            props.status?.offlineReady
              ? "text-emerald-300 font-medium"
              : props.status?.engine.healthy
                ? "text-amber-300 font-medium"
                : "text-red-400 font-medium"
          }>
            {t("app.offlineMode")}
          </span>
          <span class="text-slate-500">|</span>
          <span class="text-slate-300">
            {props.status
              ? `${props.status.engine.id.toUpperCase()}: ${props.status.providers
                  .filter((p) => p.available)
                  .map((p) => p.id.toUpperCase())
                  .join(" / ") || "No Provider"}`
              : "Initializing..."}
          </span>
        </div>

        <button
          onClick={props.onOpenSettings}
          class="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 border border-slate-700 transition shadow-sm"
          title={t("header.settings")}
        >
          <svg class="w-4 h-4 text-slate-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
          <span>{t("header.settings")}</span>
        </button>

        {/* Separator */}
        <div class="h-4 w-px bg-slate-700/80 mx-0.5"></div>

        {/* Window control buttons */}
        <div class="flex items-center space-x-1">
          {/* Minimize */}
          <button
            type="button"
            onClick={handleMinimize}
            title={t("window.minimize")}
            aria-label={t("window.minimize")}
            class="w-7 h-7 flex items-center justify-center rounded-md text-slate-400 hover:text-slate-100 hover:bg-slate-800 transition-colors"
          >
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <line x1="4" y1="12" x2="20" y2="12" />
            </svg>
          </button>

          {/* Maximize / Restore */}
          <button
            type="button"
            onClick={handleToggleMaximize}
            title={isMaximized() ? t("window.restore") : t("window.maximize")}
            aria-label={isMaximized() ? t("window.restore") : t("window.maximize")}
            class="w-7 h-7 flex items-center justify-center rounded-md text-slate-400 hover:text-slate-100 hover:bg-slate-800 transition-colors"
          >
            {isMaximized() ? (
              <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M8 4h12a1 1 0 0 1 1 1v11" />
                <rect x="3" y="8" width="13" height="13" rx="1.5" />
              </svg>
            ) : (
              <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="4" y="4" width="16" height="16" rx="2" />
              </svg>
            )}
          </button>

          {/* Close */}
          <button
            type="button"
            onClick={handleClose}
            title={t("window.close")}
            aria-label={t("window.close")}
            class="w-7 h-7 flex items-center justify-center rounded-md text-slate-400 hover:text-white hover:bg-rose-600 active:bg-rose-700 transition-colors"
          >
            <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18" stroke-linecap="round" />
              <line x1="6" y1="6" x2="18" y2="18" stroke-linecap="round" />
            </svg>
          </button>
        </div>
      </div>
    </header>
  );
};

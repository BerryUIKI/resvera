import { Component } from "solid-js";
import { RuntimeStatus } from "../types/ipc";

interface HeaderProps {
  status: RuntimeStatus | null;
  onOpenSettings: () => void;
  onOpenModelCenter: () => void;
}

export const Header: Component<HeaderProps> = (props) => {
  return (
    <header class="flex items-center justify-between px-6 py-3 bg-slate-900 border-b border-slate-800 select-none">
      <div class="flex items-center space-x-3">
        <div class="w-8 h-8 rounded-lg bg-sky-500 flex items-center justify-center font-bold text-slate-950 text-lg shadow-md">
          R
        </div>
        <div>
          <h1 class="text-base font-semibold text-slate-100 leading-tight">Resvera</h1>
          <p class="text-xs text-slate-400">Offline Local Image Upscaler</p>
        </div>
      </div>

      <div class="flex items-center space-x-3">
        <button
          onClick={props.onOpenModelCenter}
          class="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 border border-slate-700 transition"
        >
          <svg class="w-4 h-4 text-sky-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
          </svg>
          <span>Model Center</span>
        </button>

        <div class="flex items-center space-x-2 bg-slate-800/80 px-3 py-1.5 rounded-full border border-slate-700/60 text-xs">
          <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
          <span class="text-emerald-300 font-medium">100% Offline Ready</span>
          <span class="text-slate-500">|</span>
          <span class="text-slate-300">ORT: CPU / DirectML</span>
        </div>

        <button
          onClick={props.onOpenSettings}
          class="p-2 rounded-lg text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition"
          title="Settings"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
      </div>
    </header>
  );
};

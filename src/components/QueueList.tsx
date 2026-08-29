import { Component, For, Show } from "solid-js";
import { JobSnapshot } from "../types/ipc";
import { useI18n } from "../i18n";

interface QueueListProps {
  jobs: JobSnapshot[];
  selectedJobId: string | null;
  onSelectJob: (id: string) => void;
  onCancelJob: (id: string) => void;
  isPaused: boolean;
  onTogglePause: () => void;
}

export const QueueList: Component<QueueListProps> = (props) => {
  const { t } = useI18n();

  const getStatusBadge = (state: string) => {
    switch (state) {
      case "succeeded":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-emerald-950 text-emerald-400 border border-emerald-800">{t("queue.completed")}</span>;
      case "running":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-sky-950 text-sky-400 border border-sky-800 animate-pulse">{t("queue.processing")}</span>;
      case "preparing":
      case "finalizing":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-amber-950 text-amber-400 border border-amber-800">{t("queue.processing")}</span>;
      case "cancelled":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-slate-800 text-slate-400 border border-slate-700">{t("queue.cancelled")}</span>;
      case "failed":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-rose-950 text-rose-400 border border-rose-800">{t("queue.failed")}</span>;
      default:
        return <span class="px-2 py-0.5 text-[10px] rounded bg-slate-900 text-slate-400 border border-slate-800">{t("queue.queued")}</span>;
    }
  };

  return (
    <div class="flex flex-col h-full bg-slate-900 border-r border-slate-800 select-none">
      <div class="flex items-center justify-between px-4 py-3 border-b border-slate-800">
        <h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400">
          {t("queue.title")} ({props.jobs.length})
        </h2>
        <button
          onClick={props.onTogglePause}
          class="text-xs px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 transition"
        >
          {props.isPaused ? t("queue.resume") : t("queue.pause")}
        </button>
      </div>

      <div class="flex-1 overflow-y-auto divide-y divide-slate-800/60">
        <For each={props.jobs}>
          {(job) => (
            <div
              onClick={() => props.onSelectJob(job.id)}
              class={`p-3 cursor-pointer transition flex items-center justify-between ${
                props.selectedJobId === job.id
                  ? "bg-slate-800 border-l-2 border-sky-400"
                  : "hover:bg-slate-800/50"
              }`}
            >
              <div class="flex-1 min-w-0 pr-3">
                <p class="text-xs font-medium text-slate-200 truncate">
                  {job.inputPath.split(/[\\/]/).pop() || "image.png"}
                </p>
                <div class="flex items-center space-x-2 mt-1">
                  <span class="text-[10px] text-slate-500 font-mono">{job.modelId}</span>
                  <span class="text-[10px] text-sky-400 font-bold">{job.targetScale}x</span>
                </div>
              </div>

              <div class="flex items-center space-x-2">
                {getStatusBadge(job.state)}
                {(job.state === "queued" || job.state === "running") && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      props.onCancelJob(job.id);
                    }}
                    class="text-slate-500 hover:text-rose-400 p-1"
                    title={t("queue.cancel")}
                  >
                    ✕
                  </button>
                )}
              </div>
            </div>
          )}
        </For>

        <Show when={props.jobs.length === 0}>
          <div class="p-8 text-center text-xs text-slate-500">
            {t("queue.empty")}
          </div>
        </Show>
      </div>
    </div>
  );
};

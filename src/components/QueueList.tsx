import { Component, For } from "solid-js";
import { JobSnapshot } from "../types/ipc";

interface QueueListProps {
  jobs: JobSnapshot[];
  selectedJobId: string | null;
  onSelectJob: (id: string) => void;
  onCancelJob: (id: string) => void;
  isPaused: boolean;
  onTogglePause: () => void;
}

export const QueueList: Component<QueueListProps> = (props) => {
  const getStatusBadge = (state: string) => {
    switch (state) {
      case "succeeded":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-emerald-950 text-emerald-400 border border-emerald-800">Done</span>;
      case "running":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-sky-950 text-sky-400 border border-sky-800 animate-pulse">Running</span>;
      case "preparing":
      case "finalizing":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-amber-950 text-amber-400 border border-amber-800">Processing</span>;
      case "cancelled":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-slate-800 text-slate-400 border border-slate-700">Cancelled</span>;
      case "failed":
        return <span class="px-2 py-0.5 text-[10px] rounded bg-rose-950 text-rose-400 border border-rose-800">Failed</span>;
      default:
        return <span class="px-2 py-0.5 text-[10px] rounded bg-slate-900 text-slate-400 border border-slate-800">Queued</span>;
    }
  };

  return (
    <div class="flex flex-col h-full bg-slate-900 border-r border-slate-800 select-none">
      <div class="flex items-center justify-between px-4 py-3 border-b border-slate-800">
        <h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400">
          Queue ({props.jobs.length})
        </h2>
        <button
          onClick={props.onTogglePause}
          class="text-xs px-2.5 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 transition"
        >
          {props.isPaused ? "Resume Queue" : "Pause Queue"}
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
                    title="Cancel Job"
                  >
                    ✕
                  </button>
                )}
              </div>
            </div>
          )}
        </For>

        {props.jobs.length === 0 && (
          <div class="p-8 text-center text-xs text-slate-500">
            No active jobs. Drag & drop images or click Import to start.
          </div>
        )}
      </div>
    </div>
  );
};

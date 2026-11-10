import { useEffect, useRef } from "react";
import type { LogEntry } from "../types";

export function LogPanel({ logs }: { logs: LogEntry[] }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    ref.current?.scrollTo({ top: ref.current.scrollHeight });
  }, [logs]);

  const levelClass = (level: string) =>
    level === "error"
      ? "text-rose-400"
      : level === "warn"
        ? "text-amber-300"
        : "text-slate-400";

  return (
    <section className="rounded-2xl border border-line bg-panel">
      <div className="border-b border-line px-4 py-2 text-xs font-semibold uppercase tracking-wide text-slate-400">
        Activity Log
      </div>
      <div ref={ref} className="h-44 overflow-y-auto px-4 py-2 font-mono text-[11px] leading-5">
        {logs.length === 0 && <p className="text-slate-600">No activity yet.</p>}
        {logs.map((entry, i) => (
          <div key={i} className="flex gap-2">
            <span className="shrink-0 text-slate-600">
              {new Date(entry.ts).toLocaleTimeString()}
            </span>
            <span className="w-16 shrink-0 text-slate-500">[{entry.source}]</span>
            <span className={levelClass(entry.level)}>{entry.message}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

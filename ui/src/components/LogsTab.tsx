import type { AppViewState } from "../types";

export function LogsTab({ appState }: { appState: AppViewState }) {
  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Logs</h2>
      <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
        {appState.logs.length === 0 ? (
          <p className="p-3 text-sm text-slate-400">No events yet.</p>
        ) : (
          appState.logs.map((entry) => (
            <div
              className="grid grid-cols-[9rem_5rem_1fr] gap-3 border-b border-slate-800 px-3 py-2 text-sm last:border-b-0"
              key={entry.id}
            >
              <span className="text-slate-500">{entry.timestamp}</span>
              <span className="uppercase text-slate-400">{entry.source}</span>
              <span>{entry.message}</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

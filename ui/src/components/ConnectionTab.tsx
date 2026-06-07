import type { AppViewState } from "../types";

export function ConnectionTab(props: {
  appState: AppViewState;
  host: string;
  port: string;
  setHost: (value: string) => void;
  setPort: (value: string) => void;
  connect: () => void;
  disconnect: () => void;
}) {
  return (
    <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Connection</h2>
        <div className="mt-4 grid gap-3">
          <label className="grid gap-1 text-sm text-slate-300">
            Host
            <input
              className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100"
              value={props.host}
              onChange={(event) => props.setHost(event.target.value)}
              placeholder="Auto-discover"
            />
          </label>
          <label className="grid gap-1 text-sm text-slate-300">
            Port
            <input
              className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100"
              value={props.port}
              onChange={(event) => props.setPort(event.target.value)}
              placeholder="Auto"
              inputMode="numeric"
            />
          </label>
          <div className="flex gap-3">
            <button
              className="rounded-lg bg-cyan-700 px-4 py-2 font-semibold text-white hover:bg-cyan-600"
              onClick={props.connect}
            >
              Connect
            </button>
            <button
              className="rounded-lg border border-slate-700 px-4 py-2 font-semibold text-slate-100 hover:bg-slate-800"
              onClick={props.disconnect}
            >
              Disconnect
            </button>
          </div>
        </div>
      </section>
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Status</h2>
        <dl className="mt-4 grid gap-2 text-sm">
          <StatusRow label="Connection" value={props.appState.connection} />
          <StatusRow label="Scenes" value={String(props.appState.sceneCount)} />
          <StatusRow label="Channels" value={String(props.appState.channelCount)} />
          <StatusRow label="Last Event" value={props.appState.lastEventAt ?? "None"} />
        </dl>
      </section>
    </div>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-4 border-b border-slate-800 py-2 last:border-b-0">
      <dt className="text-slate-500">{label}</dt>
      <dd className="text-right text-slate-100">{value}</dd>
    </div>
  );
}

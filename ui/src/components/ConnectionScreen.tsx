import type { AppViewState, DiscoveredLv1System, Lv1SystemIdentity } from "../types";

export function ConnectionScreen(props: {
  appState: AppViewState;
  commandError: string | null;
  onDisconnect: () => void;
  onSelectSystem: (identity: Lv1SystemIdentity) => void;
  onResume: () => void;
}) {
  const isConnected = props.appState.connection === "connected";

  return (
    <main className="min-h-screen bg-slate-950 p-6 text-slate-100">
      <section className="mx-auto grid max-w-5xl gap-5">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <p className="text-sm uppercase tracking-[0.25em] text-cyan-300">LV1 Connection</p>
            <h1 className="mt-2 text-3xl font-semibold">Choose an LV1 system</h1>
            <p className="mt-2 text-slate-400">Tap a discovered system to connect.</p>
          </div>

          {isConnected && (
            <div className="flex flex-wrap gap-3">
              <button
                className="rounded-lg border border-slate-700 px-4 py-2 font-semibold text-slate-100 hover:bg-slate-800"
                onClick={props.onResume}
              >
                Resume main app
              </button>
              <button
                className="rounded-lg border border-red-800 px-4 py-2 font-semibold text-red-100 hover:bg-red-950"
                onClick={props.onDisconnect}
              >
                Disconnect
              </button>
            </div>
          )}
        </div>

        {props.commandError && (
          <p className="rounded-lg border border-red-800 bg-red-950 px-3 py-2 text-sm text-red-100">
            {props.commandError}
          </p>
        )}

        <div className="grid gap-3">
          {props.appState.discoveredLv1Systems.length === 0 ? (
            <div className="rounded-xl border border-slate-800 bg-slate-900 p-6 text-slate-400">
              Searching for LV1 systems...
            </div>
          ) : (
            props.appState.discoveredLv1Systems.map((system) => (
              <SystemRow
                key={systemKey(system)}
                system={system}
                onSelectSystem={props.onSelectSystem}
                onResume={props.onResume}
              />
            ))
          )}
        </div>
      </section>
    </main>
  );
}

function SystemRow(props: {
  system: DiscoveredLv1System;
  onSelectSystem: (identity: Lv1SystemIdentity) => void;
  onResume: () => void;
}) {
  const { system } = props;
  const isConnected = system.status === "connected";

  return (
    <button
      className="grid gap-3 rounded-xl border border-slate-800 bg-slate-900 p-5 text-left hover:border-cyan-700 hover:bg-slate-900/80 md:grid-cols-[1fr_auto] md:items-center"
      onClick={() => (isConnected ? props.onResume() : props.onSelectSystem(system.identity))}
    >
      <div>
        <div className="text-lg font-semibold text-slate-100">{system.identity.host ?? "LV1 System"}</div>
        <div className="mt-1 text-sm text-slate-400">
          {system.identity.address}:{system.identity.port}
        </div>
      </div>
      <div className="flex flex-wrap gap-2 text-sm">
        <span className="rounded-full border border-slate-700 px-3 py-1 text-slate-300">
          {system.latencyMs === null ? "Latency unknown" : `${system.latencyMs} ms`}
        </span>
        <span className="rounded-full border border-cyan-700 px-3 py-1 text-cyan-100">{system.status}</span>
      </div>
    </button>
  );
}

function systemKey(system: DiscoveredLv1System) {
  return system.identity.uuid ?? `${system.identity.address}:${system.identity.port}`;
}

import { useAppState } from "../appHooks";
import { Panel } from "./Panel";

const severityClass = {
  info: "text-console-primary",
  warning: "text-status-warning",
  error: "text-status-danger",
};

export function ConsoleLogsTab() {
  const { appState } = useAppState();

  return (
    <Panel className="h-full min-h-[32rem] overflow-hidden">
      <div className="border-b border-console-line px-4 py-3">
        <h2 className="text-base font-semibold uppercase tracking-[0.08em] text-console-primary">
          Logs
        </h2>
      </div>
      <div className="max-h-[calc(100vh-14rem)] overflow-auto p-4">
        {appState.logs.length === 0 ? (
          <p className="text-sm text-console-muted">No frontend logs yet.</p>
        ) : (
          <div className="space-y-2">
            {appState.logs.map((entry) => (
              <div
                className="grid grid-cols-[6rem_6rem_1fr] gap-3 border-b border-console-line-soft pb-2 font-mono text-sm"
                key={entry.id}
              >
                <span className="text-console-muted">{entry.timestamp}</span>
                <span className={severityClass[entry.severity]}>
                  {entry.severity.toUpperCase()}
                </span>
                <span className="font-ui text-console-primary">
                  {entry.message}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </Panel>
  );
}

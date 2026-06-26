import { useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import type { DiscoveredLv1System, Lv1SystemIdentity } from "../types";
import { ConsoleButton } from "./ConsoleButton";

type ProbeResult =
  | { status: "idle" }
  | { status: "testing" }
  | { status: "success"; tcpConnectMs: number }
  | { status: "error"; message: string };

export function ConnectionModal(props: { onResume: () => void }) {
  const { appState, commandError } = useAppState();
  const commands = useAppCommands();
  const [probeResults, setProbeResults] = useState<Record<string, ProbeResult>>(
    {},
  );

  async function testSystem(system: DiscoveredLv1System) {
    const key = systemKey(system);
    setProbeResults((current) => ({
      ...current,
      [key]: { status: "testing" },
    }));
    try {
      const result = await commands.probeLv1TcpConnectLatency(system.identity);
      setProbeResults((current) => ({
        ...current,
        [key]: { status: "success", tcpConnectMs: result.tcpConnectMs },
      }));
    } catch (error) {
      setProbeResults((current) => ({
        ...current,
        [key]: { status: "error", message: String(error) },
      }));
    }
  }

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/75 p-6 font-ui text-console-primary">
      <section className="grid h-[min(52vh,22rem)] max-h-full w-full max-w-xl grid-rows-[auto_1fr] gap-5 overflow-hidden rounded-console-panel border border-console-line bg-console-panel/95 px-6 py-6 shadow-2xl">
        <div className="flex items-start justify-between gap-6 border-b border-console-line pb-4">
          <div className="min-w-0">
            <h1 className="text-lg font-normal uppercase text-console-primary">
              Connect to LV1
            </h1>
          </div>

          <div className="flex items-center gap-3">
            {appState.connection === "connected" && (
              <ConsoleButton
                onClick={commands.disconnect}
                size="small"
                variant="ghost-danger"
              >
                Disconnect
              </ConsoleButton>
            )}
            <button
              aria-label="Close connection modal"
              className="relative h-7 w-7 text-console-secondary hover:text-console-primary"
              onClick={props.onResume}
            >
              <span className="absolute top-1/2 left-1/2 h-6 w-0.5 -translate-x-1/2 -translate-y-1/2 rotate-45 rounded-full bg-current" />
              <span className="absolute top-1/2 left-1/2 h-6 w-0.5 -translate-x-1/2 -translate-y-1/2 -rotate-45 rounded-full bg-current" />
            </button>
          </div>
        </div>

        <div className="grid min-h-0 grid-rows-[auto_1fr] gap-3">
          {commandError && (
            <p className="rounded-console-control border border-status-danger bg-console-section px-3 py-2 text-sm text-status-danger">
              {commandError}
            </p>
          )}

          <div className="grid min-h-0 content-start gap-3 overflow-auto">
            {appState.discoveredLv1Systems.length === 0 ? (
              <div className="rounded-console-panel border border-console-line bg-console-section p-5 text-base text-console-secondary">
                Searching for consoles...
              </div>
            ) : (
              appState.discoveredLv1Systems.map((system) => (
                <SystemRow
                  connectedIdentity={appState.connectedLv1Identity}
                  key={systemKey(system)}
                  onTestSystem={testSystem}
                  system={system}
                  onSelectSystem={commands.selectSystem}
                  onResume={props.onResume}
                  probeResult={
                    probeResults[systemKey(system)] ?? { status: "idle" }
                  }
                />
              ))
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

function SystemRow(props: {
  connectedIdentity: Lv1SystemIdentity | null;
  onTestSystem: (system: DiscoveredLv1System) => void | Promise<void>;
  probeResult: ProbeResult;
  system: DiscoveredLv1System;
  onSelectSystem: (identity: Lv1SystemIdentity) => void;
  onResume: () => void;
}) {
  const { system } = props;
  const isConnected = identitiesMatch(system.identity, props.connectedIdentity);
  const isUnavailable = system.status === "unavailable";
  const rowClass = isConnected
    ? "border-status-current bg-console-section/70 hover:border-status-current hover:bg-console-control/70"
    : isUnavailable
      ? "cursor-not-allowed border-console-line bg-console-section/40 opacity-70"
      : "border-console-line bg-console-section/70 hover:border-console-line-strong hover:bg-console-control/70";

  return (
    <div
      className={`grid gap-3 rounded-console-control border px-4 py-2.5 text-left md:grid-cols-[1fr_auto_auto] md:items-center ${rowClass}`}
      onClick={() => {
        if (isConnected) {
          props.onResume();
          return;
        }
        if (!isUnavailable) {
          props.onSelectSystem(system.identity);
        }
      }}
      onKeyDown={(event) => {
        if (event.key !== "Enter" && event.key !== " ") {
          return;
        }
        event.preventDefault();
        if (isConnected) {
          props.onResume();
          return;
        }
        if (!isUnavailable) {
          props.onSelectSystem(system.identity);
        }
      }}
      role="button"
      tabIndex={0}
    >
      <div className="grid min-w-0 grid-cols-[auto_1fr] items-center gap-x-3 gap-y-0.5">
        <span
          className={
            isUnavailable
              ? "row-span-2 h-2 w-2 rounded-full bg-status-danger"
              : isConnected
                ? "row-span-2 h-2 w-2 rounded-full bg-status-current"
                : "row-span-2 h-2 w-2 rounded-full bg-status-cued"
          }
        />
        <div className="truncate text-base font-normal text-console-primary">
          {system.identity.host ?? "LV1 Console"}
        </div>
        <div className="font-mono text-xs text-console-secondary">
          {system.identity.address}:{system.identity.port}
        </div>
      </div>
      <div className="flex items-center gap-3 font-mono text-sm md:justify-self-end">
        <span
          className={
            isUnavailable
              ? "text-status-danger"
              : isConnected
                ? "text-status-current"
                : "text-status-cued"
          }
        >
          {isUnavailable
            ? "Unavailable"
            : isConnected
              ? "Connected"
              : "Available"}
        </span>
        <span className="h-4 border-l border-console-line" />
        <span className="text-console-secondary">
          {probeLabel(props.probeResult)}
        </span>
        <ConsoleButton
          aria-label="Test TCP latency"
          onClick={(event) => {
            event.stopPropagation();
            void props.onTestSystem(system);
          }}
          size="small"
          variant="secondary"
        >
          Test
        </ConsoleButton>
      </div>
      <span className="h-2.5 w-2.5 rotate-45 border-t-2 border-r-2 border-console-secondary md:justify-self-end" />
    </div>
  );
}

function probeLabel(result: ProbeResult) {
  switch (result.status) {
    case "testing":
      return "Testing...";
    case "success":
      return `TCP ${result.tcpConnectMs} ms`;
    case "error":
      return result.message;
    case "idle":
      return "Not tested";
  }
}

function identitiesMatch(
  system: Lv1SystemIdentity,
  connected: Lv1SystemIdentity | null,
) {
  if (!connected) {
    return false;
  }
  if (system.uuid && connected.uuid) {
    return system.uuid === connected.uuid;
  }
  return (
    system.host === connected.host &&
    system.address === connected.address &&
    system.port === connected.port
  );
}

function systemKey(system: DiscoveredLv1System) {
  return (
    system.identity.uuid ?? `${system.identity.address}:${system.identity.port}`
  );
}

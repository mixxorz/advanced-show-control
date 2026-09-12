import { useRef, useState } from "react";
import { useAppCommands, useAppState } from "../appHooks";
import type { DiscoveredLv1System, Lv1SystemIdentity } from "../types";
import { ConsoleButton } from "./ConsoleButton";

/**
 * @cc [owner:mixxorz,label:product] connection-actions-from-projection
 * The dialog MUST derive discovered and connected console state from the projected app snapshot,
 * expose explicit disconnect only while connected, and use `onResume` only to close/resume the UI.
 * It MUST NOT initiate discovery retries or transport reconnect attempts.
 */
export function ConnectionModal(props: { onResume: () => void }) {
  const { appState, commandError } = useAppState();
  const commands = useAppCommands();
  const selectionPending = useRef(false);
  const [pendingIdentity, setPendingIdentity] =
    useState<Lv1SystemIdentity | null>(null);

  async function selectSystem(identity: Lv1SystemIdentity) {
    if (selectionPending.current) return;
    selectionPending.current = true;
    setPendingIdentity(identity);
    try {
      await commands.selectSystem(identity);
    } finally {
      selectionPending.current = false;
      setPendingIdentity(null);
    }
  }

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/75 p-6 font-ui text-console-primary">
      <section
        aria-labelledby="connection-modal-title"
        aria-modal="true"
        className="grid h-[min(52vh,22rem)] max-h-full w-full max-w-xl grid-rows-[auto_1fr] gap-5 overflow-hidden rounded-console-panel border border-console-line bg-console-panel/95 px-6 py-6 shadow-2xl"
        role="dialog"
      >
        <div className="flex items-start justify-between gap-6 border-b border-console-line pb-4">
          <div className="min-w-0">
            <h1
              className="text-lg font-normal uppercase text-console-primary"
              id="connection-modal-title"
            >
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
              autoFocus
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
                  system={system}
                  onProbeLatency={commands.probeLv1TcpConnectLatency}
                  onSelectSystem={selectSystem}
                  pendingIdentity={pendingIdentity}
                  onResume={props.onResume}
                />
              ))
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

type LatencyState =
  | { status: "idle" }
  | { status: "pending" }
  | { status: "success"; latencyMs: number }
  | { status: "error"; message: string };

/**
 * @cc [owner:mixxorz,label:safety;product] console-selection-gates
 * An unavailable console MUST NOT be selected. Selecting the already connected identity MUST only
 * resume the UI. Across all rows, only one available-console connection request MAY execute at a
 * time; every other available row MUST remain blocked until that request settles.
 */
/**
 * @cc [owner:mixxorz,label:product] latency-probe-isolation
 * A latency probe MUST be row-local and single-flight, MUST not select or connect the console, and
 * MUST replace its pending state with either the measured TCP latency or an accessible error.
 */
function SystemRow(props: {
  connectedIdentity: Lv1SystemIdentity | null;
  system: DiscoveredLv1System;
  onProbeLatency: (
    identity: Lv1SystemIdentity,
  ) => Promise<{ tcpConnectMs: number }>;
  onSelectSystem: (identity: Lv1SystemIdentity) => Promise<void>;
  onResume: () => void;
  pendingIdentity: Lv1SystemIdentity | null;
}) {
  const { system } = props;
  const [latency, setLatency] = useState<LatencyState>({ status: "idle" });
  const probePending = useRef(false);
  const displayName = system.identity.host ?? "LV1 Console";
  const isConnected = identitiesMatch(system.identity, props.connectedIdentity);
  const isUnavailable = system.status === "unavailable";
  const selectPending = props.pendingIdentity !== null;
  const isSelecting = identitiesMatch(system.identity, props.pendingIdentity);
  const rowClass = isConnected
    ? "border-status-current bg-console-section/70"
    : isUnavailable
      ? "border-console-line bg-console-section/40 opacity-70"
      : "border-console-line bg-console-section/70";

  function selectSystem() {
    if (isUnavailable) return;
    if (isConnected) {
      props.onResume();
      return;
    }
    if (selectPending) return;
    void props.onSelectSystem(system.identity);
  }

  async function probeLatency() {
    if (probePending.current) return;
    probePending.current = true;
    setLatency({ status: "pending" });
    try {
      const result = await props.onProbeLatency(system.identity);
      setLatency({ status: "success", latencyMs: result.tcpConnectMs });
    } catch (error) {
      setLatency({
        status: "error",
        message: `Latency test failed: ${String(error)}`,
      });
    } finally {
      probePending.current = false;
    }
  }

  return (
    <div
      className={`grid gap-2 rounded-console-control border p-2 md:grid-cols-[1fr_auto] md:items-center ${rowClass}`}
    >
      <button
        aria-label={`Select ${displayName}`}
        className={`grid min-w-0 gap-3 rounded-console-control px-2 py-0.5 text-left md:grid-cols-[1fr_auto_auto] md:items-center ${rowClass} ${
          isUnavailable ? "cursor-not-allowed" : "hover:bg-console-control/70"
        }`}
        disabled={isUnavailable || (selectPending && !isConnected)}
        onClick={() => void selectSystem()}
        type="button"
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
            {displayName}
          </div>
          <div className="font-mono text-xs text-console-secondary">
            {system.identity.address}:{system.identity.port}
          </div>
        </div>
        <span
          className={`font-mono text-sm ${
            isUnavailable
              ? "text-status-danger"
              : isConnected
                ? "text-status-current"
                : "text-status-cued"
          }`}
        >
          {isUnavailable
            ? "Unavailable"
            : isConnected
              ? "Connected"
              : isSelecting
                ? "Connecting…"
                : "Available"}
        </span>
        <span className="h-2.5 w-2.5 rotate-45 border-t-2 border-r-2 border-console-secondary md:justify-self-end" />
      </button>

      <div className="grid min-w-28 grid-cols-[1fr_auto] items-center gap-2 font-mono text-xs md:justify-self-end">
        <LatencyResult state={latency} />
        <ConsoleButton
          aria-label={`Test latency for ${displayName}`}
          disabled={latency.status === "pending"}
          onClick={() => void probeLatency()}
          size="small"
          variant="secondary"
        >
          Test
        </ConsoleButton>
      </div>
    </div>
  );
}

function LatencyResult(props: { state: LatencyState }) {
  if (props.state.status === "error") {
    return (
      <span className="text-status-danger" role="alert">
        {props.state.message}
      </span>
    );
  }

  const label =
    props.state.status === "idle"
      ? "Not tested"
      : props.state.status === "pending"
        ? "Testing…"
        : `${props.state.latencyMs} ms`;

  return (
    <span aria-live="polite" className="text-console-secondary" role="status">
      {label}
    </span>
  );
}

/**
 * @cc [owner:mixxorz,label:product] connected-identity-match
 * Two identities MUST match by UUID when both UUIDs exist; otherwise host, address, and port MUST
 * all match. A missing connected identity MUST never match.
 */
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

/**
 * @cc [owner:mixxorz,label:product] discovery-row-identity
 * The row key MUST change when UUID, host, address, or port changes so row-local pending and latency
 * state cannot carry over to a different discovered endpoint.
 */
function systemKey(system: DiscoveredLv1System) {
  const { uuid, host, address, port } = system.identity;
  return [uuid ?? "", host ?? "", address, port].join("\u0000");
}

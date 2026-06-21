import { useEffect, useMemo, useState } from "react";
import { ConsoleButton } from "../components/ConsoleButton";
import { Panel } from "../components/Panel";
import type { AppViewState, Lv1SystemIdentity } from "../types";
import type { SmokeResult, SmokeStepResult } from "./smokeTypes";
import type { SmokeTestParams } from "./commands";

function StepList(props: { title: string; steps: SmokeStepResult[] }) {
  return (
    <section className="rounded-console-panel border border-console-line bg-console-section p-3">
      <h3 className="text-sm font-semibold uppercase tracking-wide text-console-muted">
        {props.title}
      </h3>
      <ul className="mt-2 space-y-1 text-sm">
        {props.steps.map((step) => (
          <li key={step.step} data-ok={step.ok}>
            {step.ok ? "PASS" : "FAIL"}: {step.step} - {step.message}
          </li>
        ))}
      </ul>
    </section>
  );
}

function SmokeResults(props: { result: SmokeResult | null }) {
  if (!props.result) return null;

  return (
    <div className="mt-4 grid gap-4 lg:grid-cols-2">
      <StepList
        title="Backend Steps"
        steps={props.result.backend?.steps ?? []}
      />
      <StepList title="Projector Steps" steps={props.result.projector} />
    </div>
  );
}

export function SmokeTestPanel(props: {
  appState: AppViewState;
  onRefreshLv1Discovery: () => Promise<void>;
  onRunConnectionTest: (identity: Lv1SystemIdentity) => void | Promise<void>;
  onRunSceneRecallTest: (params: SmokeTestParams) => void | Promise<void>;
  onRunFadeStartsTest: (params: SmokeTestParams) => void | Promise<void>;
  onRunFadeCompletesTest: (params: SmokeTestParams) => void | Promise<void>;
  onRunDecreasingXFadeTest: (params: SmokeTestParams) => void | Promise<void>;
  onRunLockoutBlocksRecallTest: (
    params: SmokeTestParams,
  ) => void | Promise<void>;
  smokeResult?: SmokeResult | null;
}) {
  const [identityDraft, setIdentityDraft] = useState({
    address: "",
    uuid: "",
    host: "",
    port: "12345",
  });
  const [identityTouched, setIdentityTouched] = useState(false);
  const [sceneA, setSceneA] = useState("");
  const [sceneB, setSceneB] = useState("");
  const [group, setGroup] = useState("");
  const [channel, setChannel] = useState("");
  const [tolerance, setTolerance] = useState("0.5");
  const [minimumMovement, setMinimumMovement] = useState("3.0");
  const [timeoutMs, setTimeoutMs] = useState("15000");
  const [sampleIntervalMs, setSampleIntervalMs] = useState("250");
  const [ack, setAck] = useState(false);
  const [discoveryError, setDiscoveryError] = useState<string | null>(null);
  const { onRefreshLv1Discovery } = props;

  const discoveredIdentity = props.appState.discoveredLv1Systems[0]?.identity;
  const effectiveIdentity = useMemo(() => {
    if (identityTouched || !discoveredIdentity) return identityDraft;

    return {
      uuid: discoveredIdentity.uuid ?? "",
      host: discoveredIdentity.host ?? "",
      address: discoveredIdentity.address,
      port: String(discoveredIdentity.port),
    };
  }, [discoveredIdentity, identityDraft, identityTouched]);

  function markIdentityManualEdit(
    field: keyof typeof identityDraft,
    value: string,
  ) {
    setIdentityTouched(true);
    setIdentityDraft((current) => ({ ...current, [field]: value }));
  }

  function discoveryStatus() {
    if (identityTouched) return "Manual LV1 identity entry active.";
    if (discoveryError) return "LV1 discovery failed; enter identity manually.";
    if (discoveredIdentity) {
      const displayHost =
        effectiveIdentity.host.trim().length > 0
          ? effectiveIdentity.host.trim()
          : "discovered host";
      return `Auto-filled from discovered LV1: ${displayHost} ${effectiveIdentity.address.trim()}:${effectiveIdentity.port.trim()}`;
    }
    return "Searching for LV1 systems...";
  }

  useEffect(() => {
    let cancelled = false;

    async function refresh() {
      try {
        await onRefreshLv1Discovery();
        if (!cancelled) setDiscoveryError(null);
      } catch {
        if (!cancelled) setDiscoveryError("failed");
      }
    }

    void refresh();
    const timer = window.setInterval(() => void refresh(), 3000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [onRefreshLv1Discovery]);

  const parsedPort = Number(effectiveIdentity.port);
  const canRunConnection =
    effectiveIdentity.address.trim().length > 0 && parsedPort > 0 && ack;
  const canRunSceneTests = ack && sceneA.trim().length > 0;
  const canRunDualSceneTests = canRunSceneTests && sceneB.trim().length > 0;
  const canRunChannelTest = canRunSceneTests && channel.trim().length > 0;
  const identity: Lv1SystemIdentity = {
    uuid:
      effectiveIdentity.uuid.trim().length > 0
        ? effectiveIdentity.uuid.trim()
        : null,
    host:
      effectiveIdentity.host.trim().length > 0
        ? effectiveIdentity.host.trim()
        : null,
    address: effectiveIdentity.address.trim(),
    port: parsedPort,
  };
  const smokeParams: SmokeTestParams = {
    sceneAId: sceneA.trim(),
    sceneBId: sceneB.trim(),
    channel: { group: Number(group), channel: Number(channel) },
    toleranceDb: Number(tolerance),
    minimumMovementDb: Number(minimumMovement),
    timeoutMs: Number(timeoutMs),
    sampleIntervalMs: Number(sampleIntervalMs),
  };

  return (
    <Panel className="mt-6 p-4">
      <h2 className="text-lg font-semibold">Smoke Test Inputs</h2>
      <div className="mt-4 grid gap-4 md:grid-cols-2">
        <label className="block text-sm">
          LV1 UUID
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={effectiveIdentity.uuid}
            onChange={(event) =>
              markIdentityManualEdit("uuid", event.target.value)
            }
          />
        </label>
        <label className="block text-sm">
          LV1 host
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={effectiveIdentity.host}
            onChange={(event) =>
              markIdentityManualEdit("host", event.target.value)
            }
          />
        </label>
        <label className="block text-sm">
          LV1 address
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={effectiveIdentity.address}
            onChange={(event) =>
              markIdentityManualEdit("address", event.target.value)
            }
          />
        </label>
        <label className="block text-sm">
          LV1 port
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={effectiveIdentity.port}
            onChange={(event) =>
              markIdentityManualEdit("port", event.target.value)
            }
          />
        </label>
        <label className="block text-sm">
          Scene A
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={sceneA}
            onChange={(event) => setSceneA(event.target.value)}
          />
        </label>
        <label className="block text-sm">
          Scene B
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={sceneB}
            onChange={(event) => setSceneB(event.target.value)}
          />
        </label>
        <label className="block text-sm">
          Group
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={group}
            onChange={(event) => setGroup(event.target.value)}
          />
        </label>
        <label className="block text-sm">
          Channel
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={channel}
            onChange={(event) => setChannel(event.target.value)}
          />
        </label>
        <label className="block text-sm">
          Tolerance
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={tolerance}
            onChange={(event) => setTolerance(event.target.value)}
          />
        </label>
        <label className="block text-sm">
          Minimum movement
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={minimumMovement}
            onChange={(event) => setMinimumMovement(event.target.value)}
          />
        </label>
        <label className="block text-sm">
          Timeout (ms)
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={timeoutMs}
            onChange={(event) => setTimeoutMs(event.target.value)}
          />
        </label>
        <label className="block text-sm">
          Sample interval (ms)
          <input
            className="mt-1 block w-full rounded-console-control border border-console-line bg-console-section px-3 py-2 font-mono text-console-primary outline-none focus:border-console-line-strong"
            value={sampleIntervalMs}
            onChange={(event) => setSampleIntervalMs(event.target.value)}
          />
        </label>
      </div>
      <label className="mt-4 flex items-center gap-2 text-sm">
        <input
          aria-label="I understand this can move hardware faders"
          type="checkbox"
          checked={ack}
          onChange={(event) => setAck(event.target.checked)}
        />
        I understand this can move hardware faders
      </label>
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <ConsoleButton
          disabled={!canRunConnection}
          onClick={() => void props.onRunConnectionTest(identity)}
          variant="primary"
        >
          Run Connection Test
        </ConsoleButton>
        <ConsoleButton
          disabled={!canRunSceneTests}
          onClick={() => void props.onRunSceneRecallTest(smokeParams)}
          variant="secondary"
        >
          Run Scene Recall Test
        </ConsoleButton>
        <ConsoleButton
          disabled={!canRunSceneTests}
          onClick={() => void props.onRunFadeStartsTest(smokeParams)}
          variant="secondary"
        >
          Run Fade Starts Test
        </ConsoleButton>
        <ConsoleButton
          disabled={!canRunChannelTest}
          onClick={() => void props.onRunFadeCompletesTest(smokeParams)}
          variant="secondary"
        >
          Run Fade Completes Test
        </ConsoleButton>
        <ConsoleButton
          disabled={!canRunDualSceneTests}
          onClick={() => void props.onRunDecreasingXFadeTest(smokeParams)}
          variant="secondary"
        >
          Run Decreasing XFade Test
        </ConsoleButton>
        <ConsoleButton
          disabled={!canRunDualSceneTests}
          onClick={() => void props.onRunLockoutBlocksRecallTest(smokeParams)}
          variant="secondary"
        >
          Run Lockout Blocks Recall Test
        </ConsoleButton>
      </div>
      <p className="mt-4 text-sm text-console-muted">
        Current projector connection: {props.appState.connection}
      </p>
      <p className="mt-3 text-sm text-console-muted">{discoveryStatus()}</p>
      <SmokeResults result={props.smokeResult ?? null} />
    </Panel>
  );
}

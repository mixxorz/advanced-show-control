import { useEffect, useEffectEvent, useRef, useState } from "react";
import type { AppStatusListener } from "../AppRuntime";
import { disconnectedAppViewState, type AppViewState } from "../types";
import { SmokeTestPanel } from "./SmokeTestPanel";
import {
  exitSmokeApp,
  finishSmokeSuite,
  recallScene,
  reportSmokeSetup,
  runConnectionTest,
  runDecreasingXFadeTest,
  runFadeCompletesTest,
  runFadeStartsTest,
  runLockoutBlocksRecallTest,
  newShowFile,
  refreshLv1Discovery,
  runSceneRecallTest,
  setChannelScoped,
  setSceneDurationMs,
  setSmokeChannelGain,
  storeSceneConfig,
  type SmokeBackendResult,
  type SmokeTestParams,
} from "./commands";
import type { SmokeResult, SmokeStepResult } from "./smokeTypes";

const automatedSmokeParams: SmokeTestParams = {
  sceneAId: "0::Smoke A",
  sceneBId: "1::Smoke B",
  channel: { group: 0, channel: 1 },
  toleranceDb: 0.5,
  minimumMovementDb: 3,
  timeoutMs: 15000,
  sampleIntervalMs: 250,
};

const smokeSceneATargetDb = -10;
const smokeSceneBTargetDb = 0;

export type SmokeDebugServices = {
  frontendReady: () => Promise<void>;
  listenForAppStatus: (listener: AppStatusListener) => Promise<() => void>;
};

export function SmokeDebugApp(props: { services: SmokeDebugServices }) {
  const [appState, setAppState] = useState<AppViewState>(
    disconnectedAppViewState,
  );
  const [smokeResult, setSmokeResult] = useState<SmokeResult | null>(null);
  const automatedRunStarted = useRef(false);

  useEffect(() => {
    let cancelled = false;
    let unlisten: undefined | (() => void);

    async function start() {
      unlisten = await props.services.listenForAppStatus((snapshot) => {
        if (!cancelled) setAppState(snapshot);
      });
      await props.services.frontendReady();
    }

    void start();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [props.services]);

  useEffect(() => {
    void refreshLv1Discovery();
  }, []);

  const runAutomatedSmokeSuite = useEffectEvent(
    async (
      identity: AppViewState["discoveredLv1Systems"][number]["identity"],
    ) => {
      const results: SmokeBackendResult[] = [];
      try {
        results.push(
          await runSmokeCommand("connection", () =>
            runConnectionTest(identity),
          ),
        );
        await newShowFile();
        await recallScene(automatedSmokeParams.sceneAId);
        await setSmokeChannelGain(automatedSmokeParams, smokeSceneATargetDb);
        await storeSceneConfig(automatedSmokeParams.sceneAId);
        await recallScene(automatedSmokeParams.sceneBId);
        await setSmokeChannelGain(automatedSmokeParams, smokeSceneBTargetDb);
        await storeSceneConfig(automatedSmokeParams.sceneBId);
        await setChannelScoped(
          automatedSmokeParams.sceneAId,
          automatedSmokeParams.channel,
          true,
        );
        await setChannelScoped(
          automatedSmokeParams.sceneBId,
          automatedSmokeParams.channel,
          true,
        );
        await setSceneDurationMs(automatedSmokeParams.sceneAId, 1000);
        await setSceneDurationMs(automatedSmokeParams.sceneBId, 1000);
        await reportSmokeSetup(automatedSmokeParams);
        results.push(
          await runSmokeCommand("scene-recall", () =>
            runSceneRecallTest(automatedSmokeParams),
          ),
        );
        results.push(
          await runSmokeCommand("fade-starts", () =>
            runFadeStartsTest(automatedSmokeParams),
          ),
        );
        results.push(
          await runSmokeCommand("fade-completes", () =>
            runFadeCompletesTest(automatedSmokeParams, smokeSceneBTargetDb),
          ),
        );
        results.push(
          await runSmokeCommand("decreasing-xfade", () =>
            runDecreasingXFadeTest(automatedSmokeParams),
          ),
        );
        results.push(
          await runSmokeCommand("lockout-blocks-recall", () =>
            runLockoutBlocksRecallTest(automatedSmokeParams),
          ),
        );
        const failed = results.filter((result) => !result.ok);
        await finishSmokeSuite(
          failed.length === 0,
          failed.map((result) => result.testId).join(", "),
        );
      } catch (error) {
        await finishSmokeSuite(false, String(error));
      } finally {
        await exitSmokeApp();
      }
    },
  );

  useEffect(() => {
    const identity = appState.discoveredLv1Systems[0]?.identity;
    if (!identity || automatedRunStarted.current) return;
    automatedRunStarted.current = true;
    void runAutomatedSmokeSuite(identity);
  }, [appState.discoveredLv1Systems]);

  function projectorStepsFor(
    testId: string,
    snapshot: AppViewState,
  ): SmokeStepResult[] {
    return [
      {
        ok: true,
        step: "app-status-changed",
        message: `${testId} projected from state version ${snapshot.stateVersion}`,
      },
      {
        ok: true,
        step: "projected-logs",
        message: `${snapshot.logs.length} projected log entries available`,
      },
    ];
  }

  async function runSmokeCommand(
    testId: string,
    command: () => Promise<SmokeBackendResult>,
  ): Promise<SmokeBackendResult> {
    const backend = await command();
    setSmokeResult({ backend, projector: projectorStepsFor(testId, appState) });
    return backend;
  }

  return (
    <main className="min-h-screen bg-console-bg text-console-primary">
      <section className="mx-auto max-w-6xl p-6">
        <h1 className="text-2xl font-semibold">LV1 Hardware Smoke Tests</h1>
        <p className="mt-2 text-sm text-console-muted">
          Connection: {appState.connection}
        </p>
        {smokeResult?.backend ? (
          <p className="mt-2 text-sm text-console-muted">
            Latest smoke test: {smokeResult.backend.ok ? "ok" : "failed"} (
            {smokeResult.backend.testId})
          </p>
        ) : null}
        <SmokeTestPanel
          appState={appState}
          onRefreshLv1Discovery={refreshLv1Discovery}
          onRunConnectionTest={async (identity) => {
            await runSmokeCommand("connection", () =>
              runConnectionTest(identity),
            );
          }}
          onRunSceneRecallTest={async (params: SmokeTestParams) => {
            await runSmokeCommand("scene-recall", () =>
              runSceneRecallTest(params),
            );
          }}
          onRunFadeStartsTest={async (params: SmokeTestParams) => {
            await runSmokeCommand("fade-starts", () =>
              runFadeStartsTest(params),
            );
          }}
          onRunFadeCompletesTest={async (params: SmokeTestParams) => {
            await runSmokeCommand("fade-completes", () =>
              runFadeCompletesTest(params),
            );
          }}
          onRunDecreasingXFadeTest={async (params: SmokeTestParams) => {
            await runSmokeCommand("decreasing-xfade", () =>
              runDecreasingXFadeTest(params),
            );
          }}
          onRunLockoutBlocksRecallTest={async (params: SmokeTestParams) => {
            await runSmokeCommand("lockout-blocks-recall", () =>
              runLockoutBlocksRecallTest(params),
            );
          }}
          smokeResult={smokeResult}
        />
      </section>
    </main>
  );
}

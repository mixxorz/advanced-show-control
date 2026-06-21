import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import type { AppViewState, Lv1SystemIdentity } from "../types";
import "../index.css";

const sceneA = "0::Smoke A";
const sceneB = "1::Smoke B";
const group = 0;
const channel = 1;
const targetA = -10;
const targetB = 0;
const tolerance = 0.5;
const timeoutMs = 15_000;
const testNames = [
  "connection",
  "scene-recall",
  "fade-starts",
  "fade-completes",
  "decreasing-xfade",
  "lockout-blocks-recall",
];

type TestStatus = "pending" | "running" | "pass" | "fail";
type SmokeTest = { name: string; status: TestStatus; detail: string };

export function App() {
  const [tests, setTests] = useState<SmokeTest[]>(() =>
    testNames.map((name) => ({ name, status: "pending", detail: "" })),
  );
  const [suiteStatus, setSuiteStatus] = useState("Running");
  const [closeIn, setCloseIn] = useState<number>();
  const stateRef = useRef<AppViewState | undefined>(undefined);
  const startedRef = useRef(false);

  useEffect(() => {
    const unlisten = listen<AppViewState>("app-status-changed", (event) => {
      stateRef.current = event.payload;
    });
    return () => void unlisten.then((stop) => stop());
  }, []);

  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    void run({ stateRef, setTests, setSuiteStatus }).then((ok) => {
      setSuiteStatus(ok ? "PASS" : "FAIL");
      setCloseIn(30);
    });
  }, []);

  useEffect(() => {
    if (closeIn === undefined) return;
    if (closeIn <= 0) {
      void invoke("debug_smoke_exit_app");
      return;
    }
    const timer = window.setTimeout(() => setCloseIn(closeIn - 1), 1000);
    return () => window.clearTimeout(timer);
  }, [closeIn]);

  return (
    <main className="min-h-screen bg-console-bg p-6 text-console-primary">
      <section className="mx-auto max-w-3xl">
        <p className="text-sm uppercase tracking-wide text-console-muted">
          LV1 debug smoke
        </p>
        <h1 className="mt-1 text-2xl font-semibold">{suiteStatus}</h1>
        <p className="mt-2 text-sm text-console-muted">
          Report: logs/debug-smoke-report.txt
        </p>
        {closeIn !== undefined && (
          <>
            <p className="mt-3 text-sm text-console-muted">
              Closing in {closeIn}s
            </p>
            <button
              id="close-now"
              className="mt-3 rounded-console-control border border-console-line px-3 py-2 text-sm text-console-primary hover:bg-console-control-hover"
              onClick={() => void invoke("debug_smoke_exit_app")}
            >
              Close now
            </button>
          </>
        )}
        <ol className="mt-6 space-y-2">
          {tests.map((test) => (
            <li
              className="rounded-console-panel border border-console-line bg-console-panel p-3"
              key={test.name}
            >
              <div className="flex items-center justify-between gap-4">
                <span className="font-medium">{test.name}</span>
                <span className={`text-sm ${statusClass(test.status)}`}>
                  {test.status.toUpperCase()}
                </span>
              </div>
              {test.detail && (
                <p className="mt-1 text-sm text-console-muted">{test.detail}</p>
              )}
            </li>
          ))}
        </ol>
      </section>
    </main>
  );
}

async function run({
  stateRef,
  setTests,
  setSuiteStatus,
}: {
  stateRef: React.RefObject<AppViewState | undefined>;
  setTests: React.Dispatch<React.SetStateAction<SmokeTest[]>>;
  setSuiteStatus: React.Dispatch<React.SetStateAction<string>>;
}) {
  let ok = true;
  await invoke("frontend_ready");
  try {
    await test("connection", async () => {
      await invoke("refresh_lv1_discovery", { timeoutMs: 5000 });
      const identity = await waitFor(
        () => stateRef.current?.discoveredLv1Systems[0]?.identity,
        "LV1 discovery",
      );
      await invoke("connect_lv1_system", { identity });
      await waitFor(
        () => stateRef.current?.connection === "connected",
        "LV1 connected",
      );
      await waitFor(
        () =>
          stateRef.current?.sceneConfigs.some(
            (scene) => scene.sceneId === sceneA,
          ) &&
          stateRef.current.sceneConfigs.some(
            (scene) => scene.sceneId === sceneB,
          ),
        "smoke scene configs",
      );
      await log(`CONNECTED ${label(identity)}`);
    });

    await setup();
    await sleep(2500);
    await test("scene-recall", async () => {
      await invoke("recall_scene", { sceneId: sceneA });
      await waitScene(stateRef, "Smoke A");
    });
    await test("fade-starts", async () => {
      await reset(sceneA, targetA);
      await invoke("recall_scene", { sceneId: sceneB });
      await waitFor(async () => (await gain()) > targetA + 3, "fade movement");
    });
    await test("fade-completes", async () => {
      await reset(sceneA, targetA);
      await invoke("recall_scene", { sceneId: sceneB });
      await waitGain(targetB);
    });
    await test("decreasing-xfade", async () => {
      await reset(sceneA, targetA);
      for (const [durationMs, sceneId, target] of [
        [5000, sceneB, targetB],
        [3000, sceneA, targetA],
        [1000, sceneB, targetB],
        [500, sceneA, targetA],
      ] as const) {
        await log(
          `STEP decreasing-xfade ${durationMs}ms ${sceneId} target=${target}`,
        );
        await invoke("set_scene_duration_ms", { sceneId, durationMs });
        await invoke("recall_scene", { sceneId });
        await waitGain(target);
        await log(
          `STEP decreasing-xfade ${durationMs}ms PASS gain=${await gain()}`,
        );
      }
    });
    await test("lockout-blocks-recall", async () => {
      await reset(sceneA, targetA);
      await invoke("set_lockout", { enabled: true });
      let blocked = false;
      try {
        await invoke("recall_scene", { sceneId: sceneB });
      } catch (error) {
        blocked = String(error).includes("blocked");
        if (!blocked) throw error;
      }
      if (!blocked) throw new Error("recall was not blocked");
      await sleep(1000);
      await waitGain(targetA);
      await invoke("set_lockout", { enabled: false });
    });
  } catch (error) {
    ok = false;
    await log(`ERROR ${String(error)}`);
  } finally {
    await invoke("set_lockout", { enabled: false }).catch(() => undefined);
    await log(`SUITE ${ok ? "PASS" : "FAIL"}`);
    setSuiteStatus(ok ? "PASS" : "FAIL");
  }
  return ok;

  async function test(name: string, body: () => Promise<void>) {
    const started = Date.now();
    setTest(setTests, name, "running", "running");
    try {
      await body();
      const detail = `${Date.now() - started}ms`;
      setTest(setTests, name, "pass", detail);
      await log(`TEST ${name} PASS ${detail}`);
    } catch (error) {
      ok = false;
      setTest(setTests, name, "fail", String(error));
      await log(`TEST ${name} FAIL ${String(error)}`);
      throw error;
    }
  }
}

function setTest(
  setTests: React.Dispatch<React.SetStateAction<SmokeTest[]>>,
  name: string,
  status: TestStatus,
  detail: string,
) {
  setTests((tests) =>
    tests.map((test) =>
      test.name === name ? { ...test, status, detail } : test,
    ),
  );
}

function statusClass(status: TestStatus) {
  if (status === "pass") return "text-status-cued";
  if (status === "fail") return "text-status-danger";
  if (status === "running") return "text-status-warning";
  return "text-console-muted";
}

async function setup() {
  await invoke("new_show_file");
  await rawReset(0, targetA);
  await invoke("store_scene_config", { sceneId: sceneA });
  await rawReset(1, targetB);
  await invoke("store_scene_config", { sceneId: sceneB });
  for (const sceneId of [sceneA, sceneB]) {
    await invoke("set_channel_scoped", {
      sceneId,
      group,
      channel,
      scoped: true,
    });
    await invoke("set_scene_duration_ms", { sceneId, durationMs: 1000 });
  }
  await log(
    `SETUP ${sceneA}=${targetA} ${sceneB}=${targetB} channel=${group}:${channel}`,
  );
}

async function reset(sceneId: string, target: number) {
  await invoke("recall_scene", { sceneId });
  await invoke("debug_smoke_set_channel_gain", {
    group,
    channel,
    gainDb: target,
  });
  await waitGain(target);
}

async function rawReset(sceneIndex: number, target: number) {
  await invoke("debug_smoke_recall_lv1_scene", { sceneIndex });
  await invoke("debug_smoke_set_channel_gain", {
    group,
    channel,
    gainDb: target,
  });
  await waitGain(target);
}

async function waitScene(
  stateRef: React.RefObject<AppViewState | undefined>,
  name: string,
) {
  await waitFor(
    () => stateRef.current?.currentScene?.name === name,
    `scene ${name}`,
  );
}

async function waitGain(target: number) {
  let lastGain: number | undefined;
  await waitFor(
    async () => {
      lastGain = await gain();
      return Math.abs(lastGain - target) <= tolerance;
    },
    () => `gain ${target} last=${lastGain}`,
  );
}

async function gain() {
  return invoke<number>("debug_smoke_get_channel_gain", { group, channel });
}

async function waitFor<T>(
  check: () => T | Promise<T>,
  labelText: string | (() => string),
): Promise<NonNullable<T>> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await check();
    if (value) return value as NonNullable<T>;
    await sleep(250);
  }
  const label = typeof labelText === "function" ? labelText() : labelText;
  throw new Error(`timed out waiting for ${label}`);
}

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function log(line: string) {
  console.log(line);
  return invoke("debug_smoke_log", { line });
}

function label(identity: Lv1SystemIdentity) {
  return `${identity.host ?? identity.address}:${identity.port}`;
}

createRoot(document.getElementById("root")!).render(<App />);

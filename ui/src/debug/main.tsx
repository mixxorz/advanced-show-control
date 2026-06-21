import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
const tests = [
  "connection",
  "scene-recall",
  "fade-starts",
  "fade-completes",
  "decreasing-xfade",
  "lockout-blocks-recall",
].map((name) => ({ name, status: "pending", detail: "" }));
let state: AppViewState | undefined;
let suiteStatus = "Running";
let closeIn: number | undefined;

render();

document.addEventListener("click", (event) => {
  if ((event.target as HTMLElement).id === "close-now") {
    void invoke("debug_smoke_exit_app");
  }
});

void listen<AppViewState>("app-status-changed", (event) => {
  state = event.payload;
});

void run().then((ok) => startCloseCountdown(ok));

async function run() {
  let ok = true;
  await invoke("frontend_ready");
  try {
    await test("connection", async () => {
      await invoke("refresh_lv1_discovery", { timeoutMs: 5000 });
      const identity = await waitFor(
        () => state?.discoveredLv1Systems[0]?.identity,
        "LV1 discovery",
      );
      await invoke("connect_lv1_system", { identity });
      await waitFor(() => state?.connection === "connected", "LV1 connected");
      await waitFor(
        () =>
          state?.sceneConfigs.some((scene) => scene.sceneId === sceneA) &&
          state.sceneConfigs.some((scene) => scene.sceneId === sceneB),
        "smoke scene configs",
      );
      await log(`CONNECTED ${label(identity)}`);
    });

    await setup();
    await sleep(2500);
    await test("scene-recall", async () => {
      await invoke("recall_scene", { sceneId: sceneA });
      await waitScene("Smoke A");
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
        await invoke("set_scene_duration_ms", { sceneId, durationMs });
        await invoke("recall_scene", { sceneId });
        await waitGain(target);
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
    suiteStatus = ok ? "PASS" : "FAIL";
    render();
  }
  return ok;

  async function test(name: string, body: () => Promise<void>) {
    const started = Date.now();
    setTest(name, "running", "running");
    try {
      await body();
      const detail = `${Date.now() - started}ms`;
      setTest(name, "pass", detail);
      await log(`TEST ${name} PASS ${detail}`);
    } catch (error) {
      ok = false;
      setTest(name, "fail", String(error));
      await log(`TEST ${name} FAIL ${String(error)}`);
      throw error;
    }
  }
}

function setTest(name: string, status: string, detail: string) {
  const test = tests.find((entry) => entry.name === name);
  if (!test) return;
  test.status = status;
  test.detail = detail;
  render();
}

function startCloseCountdown(ok: boolean) {
  suiteStatus = ok ? "PASS" : "FAIL";
  closeIn = 30;
  render();
  const timer = window.setInterval(() => {
    closeIn = (closeIn ?? 1) - 1;
    render();
    if (closeIn <= 0) {
      window.clearInterval(timer);
      void invoke("debug_smoke_exit_app");
    }
  }, 1000);
}

function render() {
  document.body.innerHTML = `<main class="min-h-screen bg-console-bg p-6 text-console-primary">
    <section class="mx-auto max-w-3xl">
      <p class="text-sm uppercase tracking-wide text-console-muted">LV1 debug smoke</p>
      <h1 class="mt-1 text-2xl font-semibold">${suiteStatus}</h1>
      <p class="mt-2 text-sm text-console-muted">Report: logs/debug-smoke-report.txt</p>
      ${closeIn === undefined ? "" : `<p class="mt-3 text-sm text-console-muted">Closing in ${closeIn}s</p><button id="close-now" class="mt-3 rounded-console-control border border-console-line px-3 py-2 text-sm text-console-primary hover:bg-console-control-hover">Close now</button>`}
      <ol class="mt-6 space-y-2">
        ${tests
          .map(
            (
              test,
            ) => `<li class="rounded-console-panel border border-console-line bg-console-panel p-3">
              <div class="flex items-center justify-between gap-4">
                <span class="font-medium">${test.name}</span>
                <span class="text-sm ${statusClass(test.status)}">${test.status.toUpperCase()}</span>
              </div>
              ${test.detail ? `<p class="mt-1 text-sm text-console-muted">${escapeHtml(test.detail)}</p>` : ""}
            </li>`,
          )
          .join("")}
      </ol>
    </section>
  </main>`;
}

function statusClass(status: string) {
  if (status === "pass") return "text-status-cued";
  if (status === "fail") return "text-status-danger";
  if (status === "running") return "text-status-warning";
  return "text-console-muted";
}

function escapeHtml(value: string) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
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

async function waitScene(name: string) {
  await waitFor(() => state?.currentScene?.name === name, `scene ${name}`);
}

async function waitGain(target: number) {
  await waitFor(
    async () => Math.abs((await gain()) - target) <= tolerance,
    `gain ${target}`,
  );
}

async function gain() {
  return invoke<number>("debug_smoke_get_channel_gain", { group, channel });
}

async function waitFor<T>(
  check: () => T | Promise<T>,
  labelText: string,
): Promise<NonNullable<T>> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await check();
    if (value) return value as NonNullable<T>;
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${labelText}`);
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

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppSettings, AppViewState, Lv1SystemIdentity } from "../types";
import { executeSmokeLifecycle } from "./smokeLifecycle";
import { findSmokeSceneConfigs } from "./smokeScenes";
import "../index.css";

let sceneA = "";
let sceneB = "";
const group = 0;
const channel = 1;
const targetA = -10;
const targetB = 0;
const tolerance = 0.5;
const timeoutMs = 15_000;
const sameSceneDurationMs = 6_000;
const sameSceneMovementThresholdDb = 2;
const sameSceneFinishTimeoutMs = 3_000;
const tests = [
  "cue-list-create",
  "connection",
  "startup-auto-connect",
  "empty-scene-settings-defaults",
  "scene-settings-copy-paste",
  "new-session-clears-scene-settings-clipboard",
  "scene-recall",
  "rapid-scene-recall-queue",
  "fade-starts",
  "fade-completes",
  "same-scene-finish",
  "same-scene-override",
  "decreasing-duration-final-targets",
  "link-unlinked-scene",
  "lockout-blocks-recall",
].map((name) => ({ name, status: "pending", detail: "" }));
let state: AppViewState | undefined;
let discoveredIdentity: Lv1SystemIdentity | undefined;
let connectedIdentity: Lv1SystemIdentity | undefined;
let suiteStatus = "Running";
let closeIn: number | undefined;

render();

document.addEventListener("click", (event) => {
  if ((event.target as HTMLElement).id === "close-now") {
    void invoke("debug_smoke_exit_app");
  }
});

void start();

async function start() {
  await executeSmokeLifecycle<AppViewState, AppSettings>({
    listen: (receiveState) =>
      listen<AppViewState>("app-status-changed", (event) => {
        receiveState(event.payload);
      }),
    frontendReady: async () => {
      await log("START");
      await invoke("frontend_ready");
    },
    readSettings: (nextState) => nextState.settings,
    receiveState: (nextState) => {
      state = nextState;
    },
    runSuite: run,
    releaseLockout: () => invoke("set_lockout", { enabled: false }),
    restoreSettings: (settings) => invoke("replace_app_settings", { settings }),
    report: async (ok, error) => {
      suiteStatus = ok ? "PASS" : "FAIL";
      render();
      if (!ok) {
        await log(`ERROR ${String(error)}`).catch(console.error);
      }
      await log(`SUITE ${ok ? "PASS" : "FAIL"}`);
    },
    complete: startCloseCountdown,
  });
}

async function run() {
  await waitFor(() => state, "initial app state");
  await test("cue-list-create", async () => {
    const result = await invoke<{ cueList?: { id: string; name: string } }>(
      "create_cue_list",
      { name: "Smoke Cue List" },
    );
    const cueListId = result.cueList?.id;
    if (!cueListId)
      throw new Error("create_cue_list did not return a cue list");
    await waitFor(
      () =>
        state?.cueLists.some(
          (list) =>
            list.id === cueListId &&
            list.name === "Smoke Cue List" &&
            state?.activeCueListId === cueListId,
        ),
      "projected smoke cue list",
    );
    await log(`CUE_LIST_CREATED ${cueListId}`);
  });
  await test("connection", async () => {
    await invoke("refresh_lv1_discovery", { timeoutMs: 5000 });
    discoveredIdentity = await waitFor(
      () => state?.discoveredLv1Systems[0]?.identity,
      "LV1 discovery",
    );
    await invoke("connect_lv1_system", { identity: discoveredIdentity });
    connectedIdentity = await waitFor(
      () =>
        state?.connection === "connected" &&
        sameValue(state.connectedLv1Identity, discoveredIdentity)
          ? state.connectedLv1Identity
          : undefined,
      "LV1 connected to discovered identity",
    );
    if (!connectedIdentity.uuid) {
      throw new Error("startup auto-connect smoke requires an LV1 UUID");
    }
    const scenes = await waitFor(() => {
      return resolveSmokeSceneIds();
    }, "smoke scene configs");
    sceneA = scenes.sceneA;
    sceneB = scenes.sceneB;
    await log(`CONNECTED ${label(discoveredIdentity)}`);
  });
  await test("startup-auto-connect", async () => {
    const expectedIdentity = discoveredIdentity;
    if (!expectedIdentity?.uuid) {
      throw new Error("discovered LV1 identity is unavailable");
    }

    await invoke("disconnect_lv1");
    await waitFor(
      () => state?.connection === "disconnected",
      "LV1 disconnected",
    );

    await invoke("startup_auto_connect_lv1");
    const reconnected = await waitFor(
      () =>
        state?.connection === "connected" &&
        sameValue(state.connectedLv1Identity, expectedIdentity)
          ? state.connectedLv1Identity
          : undefined,
      "startup auto-connected LV1 identity",
    );
    await log(`AUTO_CONNECTED ${label(reconnected)}`);
  });

  await test("empty-scene-settings-defaults", async () => {
    await newSceneSettingsSession();
    await assertEmptySceneSettings(sceneA, "Smoke A");
    await assertEmptySceneSettings(sceneB, "Smoke B");
  });
  await test("scene-settings-copy-paste", async () => {
    await loadSceneSettingsSmokeSession();
    await invoke("store_scene_config", { internalSceneId: sceneA });
    await invoke("set_scene_scope_faders_enabled", {
      internalSceneId: sceneA,
      enabled: true,
    });
    await invoke("set_scene_scope_pan_enabled", {
      internalSceneId: sceneA,
      enabled: true,
    });
    await invoke("set_channel_scoped", {
      internalSceneId: sceneA,
      group,
      channel,
      scoped: true,
    });
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneA,
      durationMs: 1234,
    });

    const sourceBeforePaste = structuredClone(
      await waitFor(() => {
        const source = sceneConfig(sceneA);
        if (
          !source ||
          source.durationMs !== 1234 ||
          !source.scopeToggles.faders ||
          !source.scopeToggles.pan ||
          source.channelConfigs.length === 0 ||
          source.scopedChannels.length === 0
        ) {
          return undefined;
        }
        return source;
      }, "projected configured Smoke A scene settings"),
    );
    const destinationBeforePaste = await waitFor(
      () => sceneConfig(sceneB),
      "projected Smoke B scene settings",
    );
    await invoke("copy_scene_settings", { internalSceneId: sceneA });
    await waitFor(
      () => state?.sceneSettingsClipboardAvailable,
      "projected scene settings clipboard",
    );
    await invoke("save_show_file");
    await waitFor(
      () => state && !state.showFileDirty,
      "clean projected show file before paste",
    );
    await invoke("paste_scene_settings", { internalSceneId: sceneB });

    const destination = await waitFor(() => {
      const next = sceneConfig(sceneB);
      if (!next || !state?.showFileDirty) return undefined;
      if (
        next.durationMs !== sourceBeforePaste.durationMs ||
        !sameValue(next.scopeToggles, sourceBeforePaste.scopeToggles) ||
        !sameValue(next.channelConfigs, sourceBeforePaste.channelConfigs) ||
        !sameValue(next.scopedChannels, sourceBeforePaste.scopedChannels)
      ) {
        return undefined;
      }
      return next;
    }, "projected pasted Smoke B scene settings");
    const sourceAfterPaste = await waitFor(
      () => sceneConfig(sceneA),
      "projected Smoke A scene settings after paste",
    );

    if (
      destination.internalSceneId !== destinationBeforePaste.internalSceneId
    ) {
      throw new Error("paste changed Smoke B internal scene ID");
    }
    if (destination.sceneIndex !== destinationBeforePaste.sceneIndex) {
      throw new Error("paste changed Smoke B scene index");
    }
    if (destination.sceneName !== destinationBeforePaste.sceneName) {
      throw new Error("paste changed Smoke B scene name");
    }
    if (destination.durationMs !== sourceBeforePaste.durationMs) {
      throw new Error("paste did not copy Smoke A duration");
    }
    if (!sameValue(destination.scopeToggles, sourceBeforePaste.scopeToggles)) {
      throw new Error("paste did not copy Smoke A scope toggles");
    }
    if (
      !sameValue(destination.channelConfigs, sourceBeforePaste.channelConfigs)
    ) {
      throw new Error("paste did not copy Smoke A channel configs");
    }
    if (
      !sameValue(destination.scopedChannels, sourceBeforePaste.scopedChannels)
    ) {
      throw new Error("paste did not copy Smoke A scoped channels");
    }
    if (!sameValue(sourceAfterPaste, sourceBeforePaste)) {
      throw new Error("paste changed Smoke A scene settings");
    }
    if (!state?.showFileDirty) {
      throw new Error("paste did not mark the show file dirty");
    }
  });
  await test("new-session-clears-scene-settings-clipboard", async () => {
    await newSceneSettingsSession();
    await waitFor(
      () => state && !state.sceneSettingsClipboardAvailable,
      "cleared projected scene settings clipboard",
    );
  });

  await setup();
  await sleep(2500);
  await test("scene-recall", async () => {
    await invoke("recall_scene", { internalSceneId: sceneA });
    await waitScene("Smoke A");
  });
  await test("rapid-scene-recall-queue", async () => {
    try {
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneA,
        durationMs: 0,
      });
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneB,
        durationMs: 0,
      });

      await invoke("recall_scene", { internalSceneId: sceneB });
      const dispatchOrder: string[] = [];
      const second = invoke("recall_scene", { internalSceneId: sceneA }).then(
        () => {
          dispatchOrder.push("Smoke A");
        },
      );
      await sleep(10);
      const third = invoke("recall_scene", { internalSceneId: sceneB }).then(
        () => {
          dispatchOrder.push("Smoke B");
        },
      );

      await Promise.all([second, third]);
      if (dispatchOrder.join(",") !== "Smoke A,Smoke B") {
        throw new Error(`recall dispatch order was ${dispatchOrder.join(",")}`);
      }
      await waitScene("Smoke B");
    } finally {
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneA,
        durationMs: 1_000,
      });
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneB,
        durationMs: 1_000,
      });
    }
  });
  await test("fade-starts", async () => {
    await reset(sceneA, targetA);
    await invoke("recall_scene", { internalSceneId: sceneB });
    await waitFor(async () => (await gain()) > targetA + 3, "fade movement");
  });
  await test("fade-completes", async () => {
    await reset(sceneA, targetA);
    await invoke("recall_scene", { internalSceneId: sceneB });
    await waitGain(targetB);
  });
  await test("same-scene-finish", async () => {
    try {
      await setSameSceneSettings(true, 500);
      await reset(sceneA, targetA);
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneB,
        durationMs: sameSceneDurationMs,
      });
      await invoke("recall_scene", { internalSceneId: sceneB });
      await waitFor(async () => {
        const liveGain = await gain();
        return (
          liveGain >= targetA + sameSceneMovementThresholdDb &&
          liveGain < targetB - tolerance
        );
      }, "same-scene fade movement before target");

      const repeatedAt = Date.now();
      await invoke("recall_scene", { internalSceneId: sceneB });
      await waitFor(
        async () => Math.abs((await gain()) - targetB) <= tolerance,
        "same-scene exact finish",
        sameSceneFinishTimeoutMs,
      );
      await waitFor(
        () => state?.fadeState === "idle",
        "same-scene projected fade completion",
        sameSceneFinishTimeoutMs,
      );
      if (Date.now() - repeatedAt >= sameSceneDurationMs) {
        throw new Error("same-scene recall restarted the full fade duration");
      }
    } finally {
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneB,
        durationMs: 1_000,
      });
      await setSameSceneSettings(true, 500);
    }
  });
  await test("same-scene-override", async () => {
    try {
      await setSameSceneSettings(false, 500);
      await reset(sceneA, targetA);
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneB,
        durationMs: sameSceneDurationMs,
      });
      await invoke("recall_scene", { internalSceneId: sceneB });
      await waitFor(async () => {
        const liveGain = await gain();
        return (
          liveGain >= targetA + sameSceneMovementThresholdDb &&
          liveGain < targetB - tolerance
        );
      }, "same-scene override movement before repeat");

      const repeatedAt = Date.now();
      await invoke("recall_scene", { internalSceneId: sceneB });
      await sleep(1_000);
      if (Math.abs((await gain()) - targetB) <= tolerance) {
        throw new Error("disabled same-scene finishing completed immediately");
      }
      await waitFor(
        async () => Math.abs((await gain()) - targetB) <= tolerance,
        "same-scene override completion",
        sameSceneDurationMs + 5_000,
      );
      if (Date.now() - repeatedAt < sameSceneDurationMs) {
        throw new Error(
          "same-scene override did not use the full configured duration",
        );
      }
    } finally {
      await invoke("set_scene_duration_ms", {
        internalSceneId: sceneB,
        durationMs: 1_000,
      });
      await setSameSceneSettings(true, 500);
    }
  });
  await test("decreasing-duration-final-targets", async () => {
    await reset(sceneA, targetA);
    for (const [durationMs, internalSceneId, target] of [
      [5000, sceneB, targetB],
      [3000, sceneA, targetA],
      [1000, sceneB, targetB],
      [500, sceneA, targetA],
    ] as const) {
      await invoke("set_scene_duration_ms", {
        internalSceneId,
        durationMs,
      });
      await invoke("recall_scene", { internalSceneId });
      await waitGain(target);
    }
  });
  await test("link-unlinked-scene", async () => {
    const sourceInternalSceneId = await invoke<string>(
      "debug_smoke_load_unlinked_scene_session",
    );
    await waitFor(
      () =>
        state?.sceneConfigs.some(
          (scene) =>
            scene.internalSceneId === sourceInternalSceneId &&
            scene.sceneIndex === null,
        ),
      "unlinked smoke scene config",
    );
    await invoke("link_scene_config", {
      sourceInternalSceneId,
      targetSceneIndex: 0,
      overwriteExisting: true,
    });
    await waitFor(
      () =>
        state?.sceneConfigs.some(
          (scene) =>
            scene.internalSceneId === sourceInternalSceneId &&
            scene.sceneIndex === 0 &&
            scene.sceneName === "Smoke A",
        ),
      "linked smoke scene config",
    );
    const smokeScenes = findSmokeSceneConfigs(state?.sceneConfigs ?? []);
    if (smokeScenes.sceneA?.internalSceneId !== sourceInternalSceneId) {
      throw new Error("linked smoke scene did not replace Smoke A config");
    }
    sceneA = sourceInternalSceneId;
  });
  await test("lockout-blocks-recall", async () => {
    await reset(sceneA, targetA);
    await invoke("set_lockout", { enabled: true });
    let blocked = false;
    try {
      await invoke("recall_scene", { internalSceneId: sceneB });
    } catch (error) {
      blocked = String(error).includes("blocked");
      if (!blocked) throw error;
    }
    if (!blocked) throw new Error("recall was not blocked");
    await sleep(1000);
    await waitGain(targetA);
    await invoke("set_lockout", { enabled: false });
  });

  async function test(name: string, body: () => Promise<void>) {
    const started = Date.now();
    setTest(name, "running", "running");
    try {
      await body();
      const detail = `${Date.now() - started}ms`;
      setTest(name, "pass", detail);
      await log(`TEST ${name} PASS ${detail}`);
    } catch (error) {
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
  await newSceneSettingsSession();
  await rawReset(0, targetA);
  await invoke("store_scene_config", { internalSceneId: sceneA });
  await rawReset(1, targetB);
  await invoke("store_scene_config", { internalSceneId: sceneB });
  for (const internalSceneId of [sceneA, sceneB]) {
    await invoke("set_scene_scope_faders_enabled", {
      internalSceneId,
      enabled: true,
    });
    await invoke("set_channel_scoped", {
      internalSceneId,
      group,
      channel,
      scoped: true,
    });
    await invoke("set_scene_duration_ms", {
      internalSceneId,
      durationMs: 1000,
    });
  }
  await log(
    `SETUP ${sceneA}=${targetA} ${sceneB}=${targetB} channel=${group}:${channel}`,
  );
}

async function setSameSceneSettings(
  sameSceneRecallEnabled: boolean,
  sameSceneRecallThresholdMs: number,
) {
  const current = state?.settings;
  if (!current) throw new Error("projected settings are unavailable");
  const settings: AppSettings = {
    ...current,
    sameSceneRecallEnabled,
    sameSceneRecallThresholdMs,
  };

  await invoke("replace_app_settings", { settings });
  await waitFor(
    () =>
      state?.settings.sameSceneRecallEnabled === sameSceneRecallEnabled &&
      state.settings.sameSceneRecallThresholdMs === sameSceneRecallThresholdMs,
    "projected same-scene recall settings",
  );
}

async function newSceneSettingsSession() {
  const previousSceneA = sceneA;
  const previousSceneB = sceneB;
  const newShow = await invoke<{ selected_scene_internal_id: string | null }>(
    "new_show_file",
  );
  const selectedSceneInternalId = newShow.selected_scene_internal_id;
  if (!selectedSceneInternalId) {
    throw new Error("new show did not return a selected scene");
  }
  const scenes = await waitFor(() => {
    const next = resolveSmokeSceneIds();
    if (!next) return undefined;
    if (next.sceneA === previousSceneA || next.sceneB === previousSceneB) {
      return undefined;
    }
    if (next.sceneA !== selectedSceneInternalId) return undefined;
    return next;
  }, "smoke scene configs after new show");
  sceneA = scenes.sceneA;
  sceneB = scenes.sceneB;
}

async function loadSceneSettingsSmokeSession() {
  const previousSceneA = sceneA;
  const previousSceneB = sceneB;
  await invoke("debug_smoke_load_scene_settings_session");
  const scenes = await waitFor(() => {
    const next = resolveSmokeSceneIds();
    if (!next) return undefined;
    if (next.sceneA === previousSceneA || next.sceneB === previousSceneB) {
      return undefined;
    }
    return next;
  }, "scene settings smoke session");
  sceneA = scenes.sceneA;
  sceneB = scenes.sceneB;
}

async function reset(internalSceneId: string, target: number) {
  await invoke("recall_scene", { internalSceneId });
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

function resolveSmokeSceneIds() {
  if (!state) return undefined;
  const smokeScenes = findSmokeSceneConfigs(state.sceneConfigs);
  if (!smokeScenes.sceneA || !smokeScenes.sceneB) return undefined;
  return {
    sceneA: smokeScenes.sceneA.internalSceneId,
    sceneB: smokeScenes.sceneB.internalSceneId,
  };
}

function sceneConfig(internalSceneId: string) {
  return state?.sceneConfigs.find(
    (scene) => scene.internalSceneId === internalSceneId,
  );
}

async function assertEmptySceneSettings(internalSceneId: string, name: string) {
  const scene = await waitFor(
    () => sceneConfig(internalSceneId),
    `projected ${name} scene settings`,
  );
  if (
    scene.durationMs !== 0 ||
    scene.scopeToggles.faders ||
    scene.scopeToggles.pan ||
    scene.channelConfigs.length !== 0 ||
    scene.scopedChannels.length !== 0
  ) {
    throw new Error(`${name} scene settings were not empty in a new session`);
  }
}

function sameValue(left: unknown, right: unknown) {
  return JSON.stringify(left) === JSON.stringify(right);
}

async function waitFor<T>(
  check: () => T | Promise<T>,
  labelText: string,
  waitTimeoutMs = timeoutMs,
): Promise<NonNullable<T>> {
  const deadline = Date.now() + waitTimeoutMs;
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

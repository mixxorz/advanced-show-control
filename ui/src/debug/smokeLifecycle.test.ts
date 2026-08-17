import { describe, expect, test, vi } from "vitest";
import { executeSmokeLifecycle } from "./smokeLifecycle";

type Settings = { threshold: number; nested: { enabled: boolean } };
type State = { settings: Settings };

function harness(runSuite: () => Promise<void> = vi.fn(async () => undefined)) {
  const calls: string[] = [];
  let receiveState: ((state: State) => void) | undefined;
  const unlisten = vi.fn(() => calls.push("unlisten"));
  const report = vi.fn(async (ok: boolean) => {
    calls.push(ok ? "report-pass" : "report-fail");
  });
  const complete = vi.fn((ok: boolean) => calls.push(`complete-${ok}`));
  const restoreSettings = vi.fn(async () => {
    calls.push("restore");
  });

  return {
    calls,
    complete,
    report,
    restoreSettings,
    emit(state: State) {
      receiveState?.(state);
    },
    options: {
      listen: vi.fn(async (listener: (state: State) => void) => {
        calls.push("listen");
        receiveState = listener;
        return unlisten;
      }),
      frontendReady: vi.fn(async () => {
        calls.push("ready");
      }),
      readSettings: (state: State) => state.settings,
      receiveState: vi.fn(() => calls.push("state")),
      runSuite: vi.fn(async () => {
        calls.push("run");
        await runSuite();
      }),
      releaseLockout: vi.fn(async () => calls.push("unlock")),
      restoreSettings,
      report,
      complete,
    },
  };
}

describe("executeSmokeLifecycle", () => {
  test("awaits the listener before frontend_ready and restores the first settings snapshot", async () => {
    const smoke = harness();
    const initialState: State = {
      settings: { threshold: 321, nested: { enabled: true } },
    };

    smoke.options.frontendReady.mockImplementation(async () => {
      smoke.calls.push("ready");
      smoke.emit(initialState);
      initialState.settings.threshold = 999;
      initialState.settings.nested.enabled = false;
    });

    await executeSmokeLifecycle(smoke.options);

    expect(smoke.calls).toEqual([
      "listen",
      "ready",
      "state",
      "run",
      "unlock",
      "restore",
      "unlisten",
      "report-pass",
      "complete-true",
    ]);
    expect(smoke.restoreSettings).toHaveBeenCalledWith({
      threshold: 321,
      nested: { enabled: true },
    });
  });

  test("reports setup failures and always unlistens and completes", async () => {
    const failure = new Error("frontend setup failed");
    const smoke = harness();
    smoke.options.frontendReady.mockRejectedValue(failure);

    await expect(executeSmokeLifecycle(smoke.options)).resolves.toBe(false);

    expect(smoke.options.runSuite).not.toHaveBeenCalled();
    expect(smoke.report).toHaveBeenCalledWith(false, failure);
    expect(smoke.calls).toEqual([
      "listen",
      "unlock",
      "unlisten",
      "report-fail",
      "complete-false",
    ]);
  });

  test("reports cleanup failure while continuing remaining cleanup", async () => {
    const failure = new Error("lockout cleanup failed");
    const smoke = harness();
    smoke.options.frontendReady.mockImplementation(async () => {
      smoke.calls.push("ready");
      smoke.emit({ settings: { threshold: 123, nested: { enabled: true } } });
    });
    smoke.options.releaseLockout.mockImplementation(async () => {
      smoke.calls.push("unlock");
      throw failure;
    });

    await expect(executeSmokeLifecycle(smoke.options)).resolves.toBe(false);

    expect(smoke.restoreSettings).toHaveBeenCalled();
    expect(smoke.report).toHaveBeenCalledWith(false, failure);
    expect(smoke.calls.slice(-3)).toEqual([
      "unlisten",
      "report-fail",
      "complete-false",
    ]);
  });

  test("restores initial settings and reports failure when the suite throws", async () => {
    const failure = new Error("test failed");
    const smoke = harness(async () => {
      throw failure;
    });

    smoke.options.frontendReady.mockImplementation(async () => {
      smoke.calls.push("ready");
      smoke.emit({ settings: { threshold: 777, nested: { enabled: true } } });
    });

    await expect(executeSmokeLifecycle(smoke.options)).resolves.toBe(false);

    expect(smoke.restoreSettings).toHaveBeenCalledWith({
      threshold: 777,
      nested: { enabled: true },
    });
    expect(smoke.report).toHaveBeenCalledWith(false, failure);
    expect(smoke.calls.slice(-3)).toEqual([
      "unlisten",
      "report-fail",
      "complete-false",
    ]);
  });
});

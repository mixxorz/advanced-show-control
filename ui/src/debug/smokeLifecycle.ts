export type SmokeLifecycleOptions<State, Settings> = {
  listen: (listener: (state: State) => void) => Promise<() => void>;
  frontendReady: () => Promise<void>;
  readSettings: (state: State) => Settings;
  receiveState: (state: State) => void;
  runSuite: () => Promise<void>;
  releaseLockout: () => Promise<unknown>;
  restoreSettings: (settings: Settings) => Promise<void>;
  report: (ok: boolean, error?: unknown) => Promise<void>;
  complete: (ok: boolean) => void;
};

export async function executeSmokeLifecycle<State, Settings>(
  options: SmokeLifecycleOptions<State, Settings>,
): Promise<boolean> {
  let initialSettings: Settings | undefined;
  let unlisten: (() => void) | undefined;
  let failure: unknown;
  let ok = false;

  try {
    unlisten = await options.listen((nextState) => {
      initialSettings ??= structuredClone(options.readSettings(nextState));
      options.receiveState(nextState);
    });
    await options.frontendReady();
    await options.runSuite();
    ok = true;
  } catch (error) {
    failure = error;
  } finally {
    try {
      await options.releaseLockout();
    } catch (error) {
      if (ok) failure = error;
      ok = false;
    }

    if (initialSettings !== undefined) {
      try {
        await options.restoreSettings(initialSettings);
      } catch (error) {
        if (ok) failure = error;
        ok = false;
      }
    }

    try {
      unlisten?.();
    } catch (error) {
      if (ok) failure = error;
      ok = false;
    }

    try {
      await options.report(ok, failure);
    } catch {
      ok = false;
    } finally {
      options.complete(ok);
    }
  }

  return ok;
}

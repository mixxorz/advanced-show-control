export type SmokeLifecycleOptions<State, Settings> = {
  listen: (listener: (state: State) => void) => Promise<() => void>;
  frontendReady: () => Promise<void>;
  readSettings: (state: State) => Settings;
  receiveState: (state: State) => void;
  runSuite: () => Promise<void>;
  releaseLockout: () => Promise<unknown>;
  restoreSettings: (settings: Settings) => Promise<void>;
  report: (ok: boolean, error?: unknown) => Promise<void>;
  complete: (ok: boolean) => void | Promise<void>;
};

/**
 * @cc [owner:mixxorz,label:safety] smoke-cleanup-always-runs
 * Once setup is attempted, lockout release, restoration when a settings snapshot was acquired,
 * listener removal when a listener was installed, completion, and final reporting MUST be attempted
 * in that order even when the suite or an earlier cleanup step fails.
 */
/**
 * @cc [owner:mixxorz,label:errors] smoke-failure-is-sticky
 * A suite or cleanup failure MUST produce a false final report and result without replacing the
 * first failure supplied to `report`. Synchronous and asynchronous completion failures MUST be
 * caught before reporting so a successful run cannot leave a PASS report when its completion
 * failed; failure of the final report itself MUST produce a false result.
 */
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
      await options.complete(ok);
    } catch (error) {
      if (ok) failure = error;
      ok = false;
    }

    try {
      await options.report(ok, failure);
    } catch {
      ok = false;
    }
  }

  return ok;
}

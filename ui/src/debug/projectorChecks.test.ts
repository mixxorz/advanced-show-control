import { describe, expect, it } from "vitest";
import { disconnectedAppViewState, type AppViewState } from "../types";
import {
  projectorConnectionSteps,
  projectorFadeTransitionSteps,
} from "./projectorChecks";

describe("projectorConnectionSteps", () => {
  it("passes when projected connection and identity match", () => {
    const snapshot: AppViewState = {
      ...disconnectedAppViewState,
      connection: "connected",
      connectedLv1Identity: {
        uuid: "lv1",
        host: "lv1.local",
        address: "192.168.1.10",
        port: 12345,
      },
    };

    const steps = projectorConnectionSteps(
      snapshot,
      snapshot.connectedLv1Identity!,
    );

    expect(steps.every((step) => step.ok)).toBe(true);
  });

  it("marks fade transition snapshots by index", () => {
    const steps = projectorFadeTransitionSteps([
      { ...disconnectedAppViewState, fadeState: "running" },
      { ...disconnectedAppViewState, fadeState: "idle" },
    ]);

    expect(steps).toHaveLength(2);
    expect(steps[0]?.ok).toBe(true);
    expect(steps[1]?.ok).toBe(false);
  });
});

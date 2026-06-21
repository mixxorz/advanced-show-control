import type { AppViewState, Lv1SystemIdentity } from "../types";
import type { SmokeStepResult } from "./smokeTypes";

function sameIdentity(
  identity: Lv1SystemIdentity | null,
  expected: Lv1SystemIdentity,
) {
  return (
    identity?.uuid === expected.uuid &&
    identity?.host === expected.host &&
    identity?.address === expected.address &&
    identity?.port === expected.port
  );
}

export function projectorConnectionSteps(
  snapshot: AppViewState,
  expectedIdentity: Lv1SystemIdentity,
): SmokeStepResult[] {
  const identity = snapshot.connectedLv1Identity;

  return [
    {
      ok: snapshot.connection === "connected",
      step: "projector.connection",
      message:
        snapshot.connection === "connected"
          ? "Projector reported connected"
          : `Projector reported ${snapshot.connection}`,
      observed: { connection: snapshot.connection },
    },
    {
      ok: sameIdentity(identity, expectedIdentity),
      step: "projector.connectedIdentity",
      message: "Projected identity matches selected LV1 identity",
      observed: { expectedIdentity, identity },
    },
  ];
}

export function projectorFadeTransitionSteps(
  snapshots: AppViewState[],
): SmokeStepResult[] {
  return snapshots.map((snapshot, index) => ({
    ok: snapshot.fadeState === "running",
    step: `projector.fadeState.${index}`,
    message: `Snapshot ${index + 1} reported ${snapshot.fadeState}`,
    observed: { fadeState: snapshot.fadeState },
  }));
}

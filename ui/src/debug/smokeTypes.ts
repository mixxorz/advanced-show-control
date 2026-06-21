export type SmokeStepResult = {
  ok: boolean;
  step: string;
  message: string;
  observed?: unknown;
};

export type SmokeBackendResult = {
  ok: boolean;
  testId: string;
  startedAt: string;
  finishedAt: string;
  steps: SmokeStepResult[];
  observedEvents: string[];
  observedTraces: unknown[];
};

export type SmokeProjectorStepResult = SmokeStepResult;

export type SmokeResult = {
  backend: SmokeBackendResult | null;
  projector: SmokeProjectorStepResult[];
};

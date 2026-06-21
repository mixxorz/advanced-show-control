export type SmokeStepResult = {
  ok: boolean;
  step: string;
  message: string;
  observed?: unknown;
};

export type SmokeBackendResult = {
  ok: boolean;
  message: string;
  steps: SmokeStepResult[];
};

export type SmokeProjectorStepResult = SmokeStepResult;

export type SmokeResult = {
  backend: SmokeBackendResult | null;
  projector: SmokeProjectorStepResult[];
};

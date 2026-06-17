import { type KeyboardEvent, useRef, useState } from "react";
import { formatDurationSeconds } from "../format";

export function DurationInput(props: {
  sceneId: string;
  durationMs: number;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
}) {
  return (
    <DurationInputDraft
      key={`${props.sceneId}:${props.durationMs}`}
      {...props}
    />
  );
}

function DurationInputDraft(props: {
  sceneId: string;
  durationMs: number;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
}) {
  const [draft, setDraft] = useState(formatDurationSeconds(props.durationMs));
  const skipNextBlurCommit = useRef(false);

  function resetDraft() {
    setDraft(formatDurationSeconds(props.durationMs));
  }

  async function commit() {
    const trimmed = draft.trim();
    if (!trimmed) {
      resetDraft();
      return;
    }

    const seconds = Number(trimmed);
    if (!Number.isFinite(seconds)) {
      resetDraft();
      return;
    }

    if (seconds < 0) {
      resetDraft();
      return;
    }

    const nextDurationMs =
      seconds === 0
        ? 0
        : Math.round(Math.min(120, Math.max(0.1, seconds)) * 1000);
    if (nextDurationMs === props.durationMs) {
      setDraft(formatDurationSeconds(nextDurationMs));
      return;
    }

    const ok = await props.setSceneDurationMs(props.sceneId, nextDurationMs);
    if (ok) {
      setDraft(formatDurationSeconds(nextDurationMs));
    } else {
      resetDraft();
    }
  }

  function handleBlur() {
    if (skipNextBlurCommit.current) {
      skipNextBlurCommit.current = false;
      return;
    }

    commit();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      skipNextBlurCommit.current = true;
      void commit();
      event.currentTarget.blur();
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      skipNextBlurCommit.current = true;
      resetDraft();
      event.currentTarget.blur();
    }
  }

  return (
    <label className="flex w-full max-w-[9rem] flex-col gap-1 text-xs uppercase tracking-[0.12em] text-console-secondary">
      X-Fade
      <input
        className="rounded-console-control border border-console-line bg-console-control px-3 py-2 font-mono text-sm text-console-primary outline-none transition-colors focus:border-console-line-strong focus:bg-console-control-hover"
        max={120}
        min={0}
        onBlur={handleBlur}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={handleKeyDown}
        step={0.1}
        type="number"
        value={draft}
      />
      <span className="text-[11px] leading-tight text-console-muted">
        Use 0 for an immediate move. Values above 0 are clamped from 0.1 to 120
        seconds.
      </span>
    </label>
  );
}

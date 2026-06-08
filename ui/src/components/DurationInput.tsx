import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { formatDurationSeconds } from "../format";

export function DurationInput(props: {
  sceneId: string;
  durationMs: number;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<boolean>;
}) {
  const [draft, setDraft] = useState(formatDurationSeconds(props.durationMs));
  const skipNextBlurCommit = useRef(false);

  useEffect(() => {
    setDraft(formatDurationSeconds(props.durationMs));
  }, [props.sceneId, props.durationMs]);

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

    const nextDurationMs = seconds === 0 ? 0 : Math.round(Math.min(120, Math.max(0.1, seconds)) * 1000);
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
    <label className="mt-4 flex w-full max-w-xs flex-col gap-1 text-sm text-slate-300">
      Fade duration (seconds)
      <input
      className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100"
      max={120}
      min={0}
      onBlur={handleBlur}
      onChange={(event) => setDraft(event.target.value)}
      onKeyDown={handleKeyDown}
        step={0.1}
        type="number"
        value={draft}
      />
      <span className="text-xs text-slate-500">Use 0 for an immediate move. Values above 0 are clamped from 0.1 to 120 seconds.</span>
    </label>
  );
}

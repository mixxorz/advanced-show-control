import { type KeyboardEvent, useRef, useState } from "react";
import { useAppCommands } from "../appHooks";
import { formatDurationSeconds } from "../format";

function formatDurationDraft(durationMs: number) {
  return `${formatDurationSeconds(durationMs)}s`;
}

export function DurationInput(props: {
  internalSceneId: string;
  durationMs: number;
}) {
  return (
    <DurationInputDraft
      key={`${props.internalSceneId}:${props.durationMs}`}
      {...props}
    />
  );
}

function DurationInputDraft(props: {
  internalSceneId: string;
  durationMs: number;
}) {
  const commands = useAppCommands();
  const [draft, setDraft] = useState(formatDurationDraft(props.durationMs));
  const skipNextBlurCommit = useRef(false);

  function resetDraft() {
    setDraft(formatDurationDraft(props.durationMs));
  }

  async function setDurationMs(nextDurationMs: number) {
    if (nextDurationMs === props.durationMs) {
      setDraft(formatDurationDraft(nextDurationMs));
      return;
    }

    const ok = await commands.setSceneDurationMs(
      props.internalSceneId,
      nextDurationMs,
    );
    if (ok) {
      setDraft(formatDurationDraft(nextDurationMs));
    } else {
      resetDraft();
    }
  }

  function normalizeSeconds(seconds: number) {
    if (seconds === 0) return 0;
    return Math.round(Math.min(120, Math.max(0.1, seconds)) * 1000);
  }

  async function commit() {
    const trimmed = draft.trim().replace(/s$/i, "");
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

    await setDurationMs(normalizeSeconds(seconds));
  }

  function stepDuration(direction: 1 | -1) {
    const seconds = props.durationMs / 1000 + direction * 0.1;
    void setDurationMs(normalizeSeconds(Math.max(0, seconds)));
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
    <label className="flex shrink-0 items-center gap-3 text-base font-normal uppercase text-console-secondary">
      <span>X-Fade</span>
      <div className="flex min-h-11 gap-1">
        <input
          className="w-24 rounded-console-control border border-console-line bg-console-panel px-3 py-2 text-center font-mono text-[1.1rem] text-accent-orange outline-none transition-colors focus:border-console-line-strong"
          onBlur={handleBlur}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={handleKeyDown}
          type="text"
          value={draft}
        />
        <div className="flex w-11 shrink-0 flex-col gap-1">
          <button
            className="grid flex-1 place-items-center rounded-console-control border border-console-line bg-console-panel text-sm leading-none text-accent-orange hover:border-console-line-strong hover:text-accent-orange-hover active:border-accent-orange active:bg-accent-orange-active active:text-white"
            onClick={() => stepDuration(1)}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="h-3 w-3 stroke-white"
              fill="none"
              viewBox="0 0 12 12"
            >
              <path
                d="M3 7.5 6 4.5l3 3"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="2"
              />
            </svg>
          </button>
          <button
            className="grid flex-1 place-items-center rounded-console-control border border-console-line bg-console-panel text-sm leading-none text-accent-orange hover:border-console-line-strong hover:text-accent-orange-hover active:border-accent-orange active:bg-accent-orange-active active:text-white"
            onClick={() => stepDuration(-1)}
            type="button"
          >
            <svg
              aria-hidden="true"
              className="h-3 w-3 stroke-white"
              fill="none"
              viewBox="0 0 12 12"
            >
              <path
                d="M3 4.5 6 7.5l3-3"
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="2"
              />
            </svg>
          </button>
        </div>
      </div>
    </label>
  );
}

import { useRef, useState, type ReactNode } from "react";
import { useAppState } from "../appHooks";
import { replaceAppSettings } from "../commands";
import { shortcutKeysEqual, useShortcutCapture } from "../keyboard";
import type { AppSettings, KeyboardShortcut } from "../types";
import { KeyboardShortcutInput } from "./KeyboardShortcutInput";
import { Panel } from "./Panel";
import { SelectControl } from "./SelectControl";
import { StepperControl } from "./StepperControl";
import { ToggleControl } from "./ToggleControl";

/**
 * @cc [owner:mixxorz,label:product] settings-optimistic-replacement
 * Every edit MUST enqueue a complete `AppSettings` object in user-edit order and update the local
 * draft immediately. Only one replacement may execute at a time, while rapid edits MUST compose
 * from the latest unresolved draft rather than a stale projection.
 */
/**
 * @cc [owner:mixxorz,label:product] settings-replacement-failure
 * A synchronous throw or asynchronous rejection from the latest replacement MUST discard the draft
 * and surface an error; failures from superseded requests MUST NOT overwrite newer optimistic state.
 */
/**
 * @cc [owner:mixxorz,label:product] shortcut-conflict-rejection
 * A captured GO or Cue shortcut that equals the other configurable shortcut or a fixed File command
 * shortcut MUST be rejected without submitting settings and MUST identify the conflicting action.
 */
/**
 * @cc [owner:mixxorz,label:product] projected-settings-acknowledgement
 * When the full projected settings value equals the current optimistic draft, that exact draft MUST
 * be cleared as acknowledged. A projection that does not equal the current draft MUST NOT clear it,
 * including when a newer edit replaces a draft before acknowledgement processing completes.
 */
export function SettingsTab(props: {
  onReplaceSettings?: (settings: AppSettings) => void | Promise<void>;
}) {
  const { appState } = useAppState();
  const shortcutCapture = useShortcutCapture(SETTINGS_SHORTCUT_CAPTURE_OWNER);
  const [activeHelp, setActiveHelp] = useState<string | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [shortcutConflict, setShortcutConflict] = useState<{
    action: "go" | "cue";
    message: string;
  } | null>(null);
  const [draftSettings, setDraftSettings] = useState<AppSettings | null>(null);
  const replaceRequestId = useRef(0);
  const replacementActive = useRef(false);
  const replacementQueue = useRef<
    Array<{ requestId: number; settings: AppSettings }>
  >([]);
  const draftAcknowledged =
    draftSettings !== null && settingsEqual(appState.settings, draftSettings);
  if (draftAcknowledged) setDraftSettings(null);

  const settings =
    draftSettings && !draftAcknowledged ? draftSettings : appState.settings;

  function runNextReplacement() {
    if (replacementActive.current) return;
    const queued = replacementQueue.current.shift();
    if (!queued) return;

    replacementActive.current = true;
    let replacement: void | Promise<void>;
    try {
      replacement = props.onReplaceSettings
        ? props.onReplaceSettings(queued.settings)
        : replaceAppSettings(queued.settings);
    } catch (error) {
      finishReplacement(queued.requestId, error);
      return;
    }

    void Promise.resolve(replacement).then(
      () => finishReplacement(queued.requestId),
      (error: unknown) => finishReplacement(queued.requestId, error),
    );
  }

  function finishReplacement(requestId: number, error?: unknown) {
    if (error !== undefined && replaceRequestId.current === requestId) {
      setDraftSettings(null);
      setSettingsError(String(error));
    }
    replacementActive.current = false;
    runNextReplacement();
  }

  function replace(next: AppSettings) {
    const requestId = replaceRequestId.current + 1;
    replaceRequestId.current = requestId;
    setDraftSettings(next);
    setSettingsError(null);
    replacementQueue.current.push({ requestId, settings: next });
    runNextReplacement();
  }

  function update(next: (current: AppSettings) => AppSettings) {
    replace(next(settings));
  }

  function updateShortcut(action: "go" | "cue", shortcut: KeyboardShortcut) {
    const conflict = shortcutConflictLabel(action, shortcut, settings);
    if (conflict) {
      setShortcutConflict({
        action,
        message: `Already assigned to ${conflict}`,
      });
      return;
    }

    setShortcutConflict(null);
    update((current) => ({
      ...current,
      keyboardShortcuts: {
        ...current.keyboardShortcuts,
        [action]: shortcut,
      },
    }));
  }

  return (
    <div className="grid h-full min-h-0 gap-3 overflow-auto">
      <Panel className="flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center gap-4 border-b border-console-line px-4 py-3">
          <h2 className="text-lg font-normal uppercase text-console-primary">
            Settings
          </h2>
          {activeHelp ? (
            <>
              <div className="h-5 w-px bg-console-line" />
              <div className="text-sm leading-5 text-console-primary">
                {activeHelp}
              </div>
            </>
          ) : null}
          {settingsError ? (
            <>
              <div className="h-5 w-px bg-console-line" />
              <div className="text-sm leading-5 text-status-danger">
                {settingsError}
              </div>
            </>
          ) : null}
        </div>

        <div className="relative grid min-h-0 flex-1 content-start gap-8 overflow-auto p-4">
          <div className="grid content-start gap-8">
            <SettingsSection title="General">
              <SettingRow
                help="Open the last show file automatically when the app starts."
                label="Auto load last show file"
                onHelpChange={setActiveHelp}
              >
                <ToggleControl
                  label="Auto load last show file"
                  checked={settings.autoLoadLastShowFile}
                  onChange={(checked) =>
                    update((current) => ({
                      ...current,
                      autoLoadLastShowFile: checked,
                    }))
                  }
                />
              </SettingRow>
              <SettingRow
                help="Automatically save the session after any changes."
                label="Auto save sessions"
                onHelpChange={setActiveHelp}
              >
                <ToggleControl
                  label="Auto save sessions"
                  checked={settings.autoSaveSessions}
                  onChange={(checked) =>
                    update((current) => ({
                      ...current,
                      autoSaveSessions: checked,
                    }))
                  }
                />
              </SettingRow>
              <SettingRow
                help="Choose whether times are displayed with a 12-hour or 24-hour clock."
                label="Time display"
                onHelpChange={setActiveHelp}
              >
                <SelectControl
                  label="Time display"
                  options={[
                    { label: "12 hour", value: "twelveHour" },
                    { label: "24 hour", value: "twentyFourHour" },
                  ]}
                  value={settings.timeDisplay}
                  onChange={(value) =>
                    update((current) => ({
                      ...current,
                      timeDisplay: value as AppSettings["timeDisplay"],
                    }))
                  }
                />
              </SettingRow>
              <SettingRow
                help="Controls how much fader movement triggers a manual override. 10 reacts to tiny movements; 1 requires larger movement."
                label="Fader override sensitivity"
                onHelpChange={setActiveHelp}
              >
                <StepperControl
                  label="Fader override sensitivity"
                  min={1}
                  max={10}
                  value={settings.faderOverrideSensitivity}
                  onChange={(value) =>
                    update((current) => ({
                      ...current,
                      faderOverrideSensitivity: value,
                    }))
                  }
                />
              </SettingRow>
              <SettingRow
                help="When enabled, an accepted repeated recall completes that scene's active fade targets. When disabled, matching fades continue from their current values over the full scene duration."
                label="Same scene recall finishing"
                onHelpChange={setActiveHelp}
              >
                <ToggleControl
                  label="Same scene recall finishing"
                  checked={settings.sameSceneRecallEnabled}
                  onChange={(checked) =>
                    update((current) => ({
                      ...current,
                      sameSceneRecallEnabled: checked,
                    }))
                  }
                />
              </SettingRow>
              <SettingRow
                help="Suppress repeated identical LV1 scene notifications below this threshold. Other scene recall timing and safety gates are unchanged."
                label="Same scene recall threshold"
                onHelpChange={setActiveHelp}
              >
                <StepperControl
                  label="Same scene recall threshold"
                  min={0}
                  max={5000}
                  step={100}
                  value={settings.sameSceneRecallThresholdMs}
                  formatValue={(value) => `${value} ms`}
                  onChange={(value) =>
                    update((current) => ({
                      ...current,
                      sameSceneRecallThresholdMs: value,
                    }))
                  }
                />
              </SettingRow>
              <SettingRow
                help="Write detailed debug diagnostics to disk. Enable only while troubleshooting because log files can grow quickly."
                label="Extensive diagnostics"
                onHelpChange={setActiveHelp}
              >
                <ToggleControl
                  label="Extensive diagnostics"
                  checked={settings.enableExtensiveDiagnostics}
                  onChange={(checked) =>
                    update((current) => ({
                      ...current,
                      enableExtensiveDiagnostics: checked,
                    }))
                  }
                />
              </SettingRow>
            </SettingsSection>

            <SettingsSection title="Keyboard Shortcuts">
              <SettingRow label="GO" onHelpChange={setActiveHelp}>
                <KeyboardShortcutInput
                  label="GO keyboard shortcut"
                  shortcut={settings.keyboardShortcuts.go}
                  isCapturing={shortcutCapture.isCapturing("go")}
                  conflictMessage={
                    shortcutConflict?.action === "go"
                      ? shortcutConflict.message
                      : undefined
                  }
                  onStartCapture={() => {
                    setShortcutConflict(null);
                    shortcutCapture.startCapture({
                      id: "go",
                      onCapture: (shortcut) => updateShortcut("go", shortcut),
                    });
                  }}
                />
              </SettingRow>
              <SettingRow label="CUE" onHelpChange={setActiveHelp}>
                <KeyboardShortcutInput
                  label="Cue keyboard shortcut"
                  shortcut={settings.keyboardShortcuts.cue}
                  isCapturing={shortcutCapture.isCapturing("cue")}
                  conflictMessage={
                    shortcutConflict?.action === "cue"
                      ? shortcutConflict.message
                      : undefined
                  }
                  onStartCapture={() => {
                    setShortcutConflict(null);
                    shortcutCapture.startCapture({
                      id: "cue",
                      onCapture: (shortcut) => updateShortcut("cue", shortcut),
                    });
                  }}
                />
              </SettingRow>
            </SettingsSection>
          </div>
        </div>
      </Panel>
    </div>
  );
}

const SETTINGS_SHORTCUT_CAPTURE_OWNER = "settings-tab";

function settingsEqual(left: AppSettings, right: AppSettings) {
  return (
    left.autoLoadLastShowFile === right.autoLoadLastShowFile &&
    left.autoSaveSessions === right.autoSaveSessions &&
    shortcutsEqual(left.keyboardShortcuts.go, right.keyboardShortcuts.go) &&
    shortcutsEqual(left.keyboardShortcuts.cue, right.keyboardShortcuts.cue) &&
    left.timeDisplay === right.timeDisplay &&
    left.faderOverrideSensitivity === right.faderOverrideSensitivity &&
    left.enableExtensiveDiagnostics === right.enableExtensiveDiagnostics &&
    left.sameSceneRecallEnabled === right.sameSceneRecallEnabled &&
    left.sameSceneRecallThresholdMs === right.sameSceneRecallThresholdMs
  );
}

function shortcutConflictLabel(
  action: "go" | "cue",
  shortcut: KeyboardShortcut,
  settings: AppSettings,
) {
  const otherAction = action === "go" ? "cue" : "go";
  if (shortcutsEqual(shortcut, settings.keyboardShortcuts[otherAction])) {
    return otherAction === "go" ? "GO" : "Cue";
  }

  return (
    fixedShortcutConflicts().find((item) =>
      shortcutsEqual(shortcut, item.shortcut),
    )?.label ?? null
  );
}

/**
 * @cc [owner:mixxorz,label:product] shortcut-conflict-equivalence
 * Shortcut conflict comparison MUST normalize key case while requiring exact equality for Shift,
 * Control, Alt, and Meta modifiers.
 */
function shortcutsEqual(left: KeyboardShortcut, right: KeyboardShortcut) {
  return (
    shortcutKeysEqual(left.key, right.key) &&
    left.modifiers.shift === right.modifiers.shift &&
    left.modifiers.control === right.modifiers.control &&
    left.modifiers.alt === right.modifiers.alt &&
    left.modifiers.meta === right.modifiers.meta
  );
}

function fixedShortcutConflicts(): Array<{
  label: string;
  shortcut: KeyboardShortcut;
}> {
  return [
    fixedCommandShortcut("New Session", "N", false),
    fixedCommandShortcut("Open Session", "O", false),
    fixedCommandShortcut("Save Session", "S", false),
    fixedCommandShortcut("Save As", "S", true),
  ];
}

/**
 * @cc [owner:mixxorz,label:product] platform-file-shortcuts
 * Fixed File shortcut conflicts MUST use Meta on macOS and Control elsewhere, preserving Shift only
 * for commands whose accelerator requires it.
 */
function fixedCommandShortcut(
  label: string,
  key: string,
  shift: boolean,
): { label: string; shortcut: KeyboardShortcut } {
  const isMac = navigator.platform.toLowerCase().includes("mac");
  return {
    label,
    shortcut: {
      key,
      modifiers: {
        shift,
        control: !isMac,
        alt: false,
        meta: isMac,
      },
    },
  };
}

function SettingsSection(props: { title: string; children: ReactNode }) {
  return (
    <section className="grid content-start gap-2">
      <h3 className="text-xs uppercase tracking-[0.08em] text-console-primary">
        {props.title}
      </h3>
      <div className="grid content-start gap-3 md:grid-cols-[minmax(14rem,18rem)_max-content] md:items-center">
        {props.children}
      </div>
    </section>
  );
}

function SettingRow(props: {
  label: string;
  help?: string;
  children: ReactNode;
  onHelpChange?: (help: string | null) => void;
}) {
  function showHelp() {
    props.onHelpChange?.(props.help ?? null);
  }

  function hideHelp() {
    props.onHelpChange?.(null);
  }

  return (
    <div
      className="grid gap-2 md:contents"
      onBlur={hideHelp}
      onFocus={showHelp}
      onMouseEnter={showHelp}
      onMouseLeave={hideHelp}
    >
      <div className="cursor-default text-sm font-normal text-console-muted">
        {props.label}
      </div>
      <div className="flex min-h-9 items-center text-sm text-console-primary">
        {props.children}
      </div>
    </div>
  );
}

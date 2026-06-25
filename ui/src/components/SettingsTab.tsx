import { useRef, useState, type ReactNode } from "react";
import { useAppState } from "../appHooks";
import { replaceAppSettings } from "../commands";
import { useShortcutCapture } from "../keyboard";
import type { AppSettings, KeyboardShortcut } from "../types";
import { KeyboardShortcutInput } from "./KeyboardShortcutInput";
import { Panel } from "./Panel";
import { SelectControl } from "./SelectControl";
import { StepperControl } from "./StepperControl";
import { ToggleControl } from "./ToggleControl";

export function SettingsTab(props: {
  onReplaceSettings?: (settings: AppSettings) => void | Promise<void>;
}) {
  const { appState } = useAppState();
  const shortcutCapture = useShortcutCapture();
  const [activeHelp, setActiveHelp] = useState<string | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [draftSettings, setDraftSettings] = useState<AppSettings | null>(null);
  const replaceRequestId = useRef(0);
  const settings =
    draftSettings && !settingsEqual(appState.settings, draftSettings)
      ? draftSettings
      : appState.settings;

  function replace(next: AppSettings) {
    const requestId = replaceRequestId.current + 1;
    replaceRequestId.current = requestId;
    setDraftSettings(next);
    setSettingsError(null);

    const replacement = props.onReplaceSettings
      ? props.onReplaceSettings(next)
      : replaceAppSettings(next);

    void Promise.resolve(replacement).catch((error) => {
      if (replaceRequestId.current !== requestId) return;
      setDraftSettings(null);
      setSettingsError(String(error));
    });
  }

  function update(next: (current: AppSettings) => AppSettings) {
    replace(next(settings));
  }

  function updateShortcut(action: "go" | "cue", shortcut: KeyboardShortcut) {
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
                help="After GO recalls a scene, automatically cue the following scene in the active cue list."
                label="Auto cue next scene on GO"
                onHelpChange={setActiveHelp}
              >
                <ToggleControl
                  label="Auto cue next scene on GO"
                  checked={settings.autoCueNextSceneOnGo}
                  onChange={(checked) =>
                    update((current) => ({
                      ...current,
                      autoCueNextSceneOnGo: checked,
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
            </SettingsSection>

            <SettingsSection title="Keyboard Shortcuts">
              <SettingRow label="GO" onHelpChange={setActiveHelp}>
                <KeyboardShortcutInput
                  label="GO keyboard shortcut"
                  shortcut={settings.keyboardShortcuts.go}
                  isCapturing={shortcutCapture.isCapturing("go")}
                  onStartCapture={() =>
                    shortcutCapture.startCapture({
                      id: "go",
                      onCapture: (shortcut) => updateShortcut("go", shortcut),
                    })
                  }
                />
              </SettingRow>
              <SettingRow label="CUE" onHelpChange={setActiveHelp}>
                <KeyboardShortcutInput
                  label="Cue keyboard shortcut"
                  shortcut={settings.keyboardShortcuts.cue}
                  isCapturing={shortcutCapture.isCapturing("cue")}
                  onStartCapture={() =>
                    shortcutCapture.startCapture({
                      id: "cue",
                      onCapture: (shortcut) => updateShortcut("cue", shortcut),
                    })
                  }
                />
              </SettingRow>
            </SettingsSection>
          </div>
        </div>
      </Panel>
    </div>
  );
}

function settingsEqual(left: AppSettings, right: AppSettings) {
  return JSON.stringify(left) === JSON.stringify(right);
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

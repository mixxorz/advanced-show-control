import type { ReactNode } from "react";
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
  onReplaceSettings?: (settings: AppSettings) => void;
}) {
  const { appState } = useAppState();
  const settings = appState.settings;
  const shortcutCapture = useShortcutCapture();

  function replace(next: AppSettings) {
    if (props.onReplaceSettings) {
      props.onReplaceSettings(next);
      return;
    }
    void replaceAppSettings(next);
  }

  function updateShortcut(action: "go" | "cue", shortcut: KeyboardShortcut) {
    replace({
      ...settings,
      keyboardShortcuts: {
        ...settings.keyboardShortcuts,
        [action]: shortcut,
      },
    });
  }

  return (
    <div className="grid h-full min-h-0 gap-3 overflow-auto">
      <Panel className="flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center justify-between gap-3 border-b border-console-line px-4 py-3">
          <h2 className="text-lg font-normal uppercase text-console-primary">
            Settings
          </h2>
        </div>

        <div className="grid content-start gap-8 p-4">
          <SettingsSection title="General">
            <SettingRow label="Auto load last show file">
              <ToggleControl
                label="Auto load last show file"
                checked={settings.autoLoadLastShowFile}
                onChange={(checked) =>
                  replace({ ...settings, autoLoadLastShowFile: checked })
                }
              />
            </SettingRow>
            <SettingRow label="Auto save sessions">
              <ToggleControl
                label="Auto save sessions"
                checked={settings.autoSaveSessions}
                onChange={(checked) =>
                  replace({ ...settings, autoSaveSessions: checked })
                }
              />
            </SettingRow>
            <SettingRow label="Auto cue next scene on GO">
              <ToggleControl
                label="Auto cue next scene on GO"
                checked={settings.autoCueNextSceneOnGo}
                onChange={(checked) =>
                  replace({ ...settings, autoCueNextSceneOnGo: checked })
                }
              />
            </SettingRow>

            <SettingRow label="Time display">
              <SelectControl
                label="Time display"
                options={[
                  { label: "12 hour", value: "twelveHour" },
                  { label: "24 hour", value: "twentyFourHour" },
                ]}
                value={settings.timeDisplay}
                onChange={(value) =>
                  replace({
                    ...settings,
                    timeDisplay: value as AppSettings["timeDisplay"],
                  })
                }
              />
            </SettingRow>
            <SettingRow label="Fader override sensitivity">
              <StepperControl
                label="Fader override sensitivity"
                min={1}
                max={10}
                value={settings.faderOverrideSensitivity}
                onChange={(value) =>
                  replace({
                    ...settings,
                    faderOverrideSensitivity: value,
                  })
                }
              />
            </SettingRow>
          </SettingsSection>

          <SettingsSection title="Keyboard Shortcuts">
            <SettingRow label="GO keyboard shortcut">
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
            <SettingRow label="Cue keyboard shortcut">
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
      </Panel>
    </div>
  );
}

function SettingsSection(props: { title: string; children: ReactNode }) {
  return (
    <section className="grid content-start gap-2">
      <h3 className="text-xs uppercase tracking-[0.08em] text-console-primary">
        {props.title}
      </h3>
      <div className="grid content-start gap-3 md:grid-cols-[minmax(14rem,18rem)_minmax(0,1fr)] md:items-center">
        {props.children}
      </div>
    </section>
  );
}

function SettingRow(props: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-2 md:contents">
      <div className="text-sm font-normal text-console-muted">
        {props.label}
      </div>
      <div className="flex min-h-9 items-center text-sm text-console-primary">
        {props.children}
      </div>
    </div>
  );
}

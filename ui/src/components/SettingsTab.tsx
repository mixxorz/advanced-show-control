import { useAppState } from "../appHooks";
import { replaceAppSettings } from "../commands";
import type { AppSettings, KeyboardShortcut } from "../types";
import { Panel } from "./Panel";

export function SettingsTab() {
  const { appState } = useAppState();
  const settings = appState.settings;

  function replace(next: AppSettings) {
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
      <Panel className="p-4">
        <h1 className="text-lg font-semibold text-console-primary">Settings</h1>
        <p className="mt-1 text-sm text-console-muted">
          Settings are saved immediately to this computer. These controls do not
          change show behavior yet.
        </p>
      </Panel>

      <Panel className="grid gap-4 p-4">
        <SettingCheckbox
          label="Auto load last show file"
          checked={settings.autoLoadLastShowFile}
          onChange={(checked) =>
            replace({ ...settings, autoLoadLastShowFile: checked })
          }
        />
        <SettingCheckbox
          label="Auto save sessions"
          checked={settings.autoSaveSessions}
          onChange={(checked) =>
            replace({ ...settings, autoSaveSessions: checked })
          }
        />
        <SettingCheckbox
          label="Auto cue next scene on GO"
          checked={settings.autoCueNextSceneOnGo}
          onChange={(checked) =>
            replace({ ...settings, autoCueNextSceneOnGo: checked })
          }
        />
      </Panel>

      <Panel className="grid gap-4 p-4">
        <label className="grid gap-2 text-sm text-console-muted">
          Time display
          <select
            className="rounded-console-button border border-console-line bg-console-surface px-3 py-2 text-console-primary"
            value={settings.timeDisplay}
            onChange={(event) =>
              replace({
                ...settings,
                timeDisplay: event.target.value as AppSettings["timeDisplay"],
              })
            }
          >
            <option value="twelveHour">12 hour</option>
            <option value="twentyFourHour">24 hour</option>
          </select>
        </label>
        <label className="grid gap-2 text-sm text-console-muted">
          Fader override sensitivity
          <input
            aria-label="Fader override sensitivity"
            type="range"
            min="1"
            max="10"
            value={settings.faderOverrideSensitivity}
            onChange={(event) =>
              replace({
                ...settings,
                faderOverrideSensitivity: Number(event.target.value),
              })
            }
          />
          <span className="text-console-primary">
            {settings.faderOverrideSensitivity}
          </span>
        </label>
      </Panel>

      <Panel className="grid gap-4 p-4">
        <ShortcutInput
          label="GO keyboard shortcut"
          shortcut={settings.keyboardShortcuts.go}
          onChange={(shortcut) => updateShortcut("go", shortcut)}
        />
        <ShortcutModifierControls
          labelPrefix="GO"
          shortcut={settings.keyboardShortcuts.go}
          onChange={(modifiers) =>
            updateShortcut("go", {
              ...settings.keyboardShortcuts.go,
              modifiers,
            })
          }
        />
        <ShortcutInput
          label="Cue keyboard shortcut"
          shortcut={settings.keyboardShortcuts.cue}
          onChange={(shortcut) => updateShortcut("cue", shortcut)}
        />
        <ShortcutModifierControls
          labelPrefix="Cue"
          shortcut={settings.keyboardShortcuts.cue}
          onChange={(modifiers) =>
            updateShortcut("cue", {
              ...settings.keyboardShortcuts.cue,
              modifiers,
            })
          }
        />
      </Panel>
    </div>
  );
}

function SettingCheckbox(props: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 text-sm text-console-primary">
      {props.label}
      <input
        aria-label={props.label}
        type="checkbox"
        checked={props.checked}
        onChange={(event) => props.onChange(event.target.checked)}
      />
    </label>
  );
}

function ShortcutInput(props: {
  label: string;
  shortcut: KeyboardShortcut;
  onChange: (shortcut: KeyboardShortcut) => void;
}) {
  return (
    <label className="grid gap-2 text-sm text-console-muted">
      {props.label}
      <input
        className="rounded-console-button border border-console-line bg-console-surface px-3 py-2 text-console-primary"
        value={props.shortcut.key}
        onChange={(event) =>
          props.onChange({ ...props.shortcut, key: event.target.value })
        }
      />
    </label>
  );
}

function ShortcutModifierControls(props: {
  labelPrefix: string;
  shortcut: KeyboardShortcut;
  onChange: (modifiers: KeyboardShortcut["modifiers"]) => void;
}) {
  return (
    <div className="grid gap-2">
      <span className="text-sm text-console-muted">
        {props.labelPrefix} modifiers
      </span>
      <div className="flex flex-wrap gap-4 text-sm text-console-primary">
        {(["shift", "control", "alt", "meta"] as const).map((modifier) => (
          <label key={modifier} className="flex items-center gap-2">
            <input
              aria-label={`${props.labelPrefix} ${capitalize(modifier)}`}
              type="checkbox"
              checked={props.shortcut.modifiers[modifier]}
              onChange={(event) =>
                props.onChange({
                  ...props.shortcut.modifiers,
                  [modifier]: event.target.checked,
                })
              }
            />
            {capitalize(modifier)}
          </label>
        ))}
      </div>
    </div>
  );
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

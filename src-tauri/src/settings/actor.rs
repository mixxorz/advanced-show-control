use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::runtime::events::{AppEvent, AppEventBus};

use super::commands::{SettingsCommand, SettingsCommandResult};
use super::events::SettingsEvent;
use super::handle::SettingsHandle;
use super::state::SettingsState;
use super::{AppSettings, KeyboardShortcut, TimeDisplayFormat};

pub struct SettingsActorTask {
    rx: mpsc::Receiver<SettingsCommand>,
    event_bus: AppEventBus,
    state: SettingsState,
}

impl SettingsActorTask {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(run_settings_actor(self.rx, self.event_bus, self.state));
    }
}

pub fn build_settings_actor(
    settings_dir: PathBuf,
    event_bus: AppEventBus,
) -> (SettingsHandle, SettingsActorTask) {
    let (tx, rx) = mpsc::channel(32);
    let state = SettingsState::load(settings_dir);
    let task = SettingsActorTask {
        rx,
        event_bus,
        state,
    };
    (SettingsHandle::new(tx), task)
}

async fn run_settings_actor(
    mut rx: mpsc::Receiver<SettingsCommand>,
    event_bus: AppEventBus,
    mut state: SettingsState,
) {
    while let Some(command) = rx.recv().await {
        handle_command(command, &event_bus, &mut state).await;
    }
    tracing::debug!(event = "settings_actor_stopped", "Settings actor stopped");
}

async fn handle_command(
    command: SettingsCommand,
    event_bus: &AppEventBus,
    state: &mut SettingsState,
) {
    match command {
        SettingsCommand::GetSettings { reply } => {
            let _ = reply.send(state.settings());
        }
        SettingsCommand::ReplaceSettings { settings, reply } => {
            let result = state.replace_settings(settings).map(|changed| {
                if changed {
                    let settings = state.settings();
                    log_settings_updated(&settings);
                    event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged { settings }));
                } else {
                    tracing::debug!(
                        event = "settings_update_noop",
                        "Settings already match requested values"
                    );
                }
                SettingsCommandResult { changed }
            });
            let _ = reply.send(result);
        }
    }
}

fn log_settings_updated(settings: &AppSettings) {
    tracing::info!(
        event = "settings_updated",
        auto_load_last_show_file = settings.auto_load_last_show_file,
        auto_save_sessions = settings.auto_save_sessions,
        auto_cue_next_scene_on_go = settings.auto_cue_next_scene_on_go,
        time_display = time_display_label(&settings.time_display),
        fader_override_sensitivity = settings.fader_override_sensitivity,
        go_shortcut = %shortcut_label(&settings.keyboard_shortcuts.go),
        cue_shortcut = %shortcut_label(&settings.keyboard_shortcuts.cue),
        "Settings updated"
    );
}

fn time_display_label(value: &TimeDisplayFormat) -> &'static str {
    match value {
        TimeDisplayFormat::TwelveHour => "twelve_hour",
        TimeDisplayFormat::TwentyFourHour => "twenty_four_hour",
    }
}

fn shortcut_label(shortcut: &KeyboardShortcut) -> String {
    let mut parts = Vec::new();
    if shortcut.modifiers.shift {
        parts.push("Shift");
    }
    if shortcut.modifiers.control {
        parts.push("Control");
    }
    if shortcut.modifiers.alt {
        parts.push("Alt");
    }
    if shortcut.modifiers.meta {
        parts.push("Meta");
    }
    parts.push(shortcut.key.as_str());
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::{SettingsCommand, SettingsCommandResult, SettingsHandle, build_settings_actor};
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::settings::{AppSettings, SettingsEvent};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::oneshot;

    fn temp_settings_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("asc-settings-{name}-{unique}"))
    }

    async fn get_settings(handle: &SettingsHandle) -> AppSettings {
        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetSettings { reply })
            .await
            .unwrap();
        rx.await.unwrap()
    }

    #[tokio::test]
    async fn actor_loads_defaults_when_file_is_missing() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("missing");
        let (handle, task) = build_settings_actor(dir, event_bus);
        task.spawn();

        assert_eq!(get_settings(&handle).await, AppSettings::default());
    }

    #[tokio::test]
    async fn actor_loads_defaults_when_file_is_invalid() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("invalid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), "not json").unwrap();
        let (handle, task) = build_settings_actor(dir, event_bus);
        task.spawn();

        assert_eq!(get_settings(&handle).await, AppSettings::default());
    }

    #[tokio::test]
    async fn actor_loads_partial_file_with_defaults() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("partial");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.json"),
            r#"{"autoSaveSessions":true,"keyboardShortcuts":{"cue":{"key":"K"}}}"#,
        )
        .unwrap();
        let (handle, task) = build_settings_actor(dir, event_bus);
        task.spawn();

        let settings = get_settings(&handle).await;
        assert!(settings.auto_save_sessions);
        assert_eq!(settings.keyboard_shortcuts.go.key, "Space");
        assert_eq!(settings.keyboard_shortcuts.cue.key, "K");
        assert_eq!(settings.fader_override_sensitivity, 9);
    }

    #[tokio::test]
    async fn actor_normalizes_replacement_saves_file_and_publishes_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let dir = temp_settings_dir("replace");
        let (handle, task) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::ReplaceSettings {
                settings: AppSettings {
                    auto_save_sessions: true,
                    fader_override_sensitivity: 99,
                    ..Default::default()
                },
                reply,
            })
            .await
            .unwrap();

        assert_eq!(
            rx.await.unwrap().unwrap(),
            SettingsCommandResult { changed: true }
        );
        let saved = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(saved.contains("autoSaveSessions"));
        assert!(saved.contains("\"faderOverrideSensitivity\": 10"));

        let received = events.recv().await.unwrap();
        assert!(matches!(
            received,
            AppEvent::Settings(SettingsEvent::StateChanged { settings })
                if settings.auto_save_sessions && settings.fader_override_sensitivity == 10
        ));
    }

    #[tokio::test]
    async fn actor_does_not_save_or_publish_when_normalized_settings_are_unchanged() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let dir = temp_settings_dir("unchanged");
        let (handle, task) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::ReplaceSettings {
                settings: AppSettings::default(),
                reply,
            })
            .await
            .unwrap();

        assert_eq!(
            rx.await.unwrap().unwrap(),
            SettingsCommandResult { changed: false }
        );
        assert!(!dir.join("settings.json").exists());
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), events.recv())
                .await
                .is_err()
        );
    }
}

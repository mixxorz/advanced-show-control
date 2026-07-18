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
    #[cfg(test)]
    set_last_connected_lv1_gate: Option<SetLastConnectedLv1Gate>,
}

impl SettingsActorTask {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(run_settings_actor(
            self.rx,
            self.event_bus,
            self.state,
            #[cfg(test)]
            self.set_last_connected_lv1_gate,
        ));
    }

    #[cfg(test)]
    pub(crate) fn pause_set_last_connected_lv1(
        mut self,
        received: tokio::sync::oneshot::Sender<()>,
        release: tokio::sync::oneshot::Receiver<()>,
    ) -> Self {
        self.set_last_connected_lv1_gate = Some(SetLastConnectedLv1Gate { received, release });
        self
    }
}

#[cfg(test)]
struct SetLastConnectedLv1Gate {
    received: tokio::sync::oneshot::Sender<()>,
    release: tokio::sync::oneshot::Receiver<()>,
}

pub fn build_settings_actor(
    settings_dir: PathBuf,
    event_bus: AppEventBus,
) -> (SettingsHandle, SettingsActorTask, AppSettings) {
    let (tx, rx) = mpsc::channel(32);
    let state = SettingsState::load(settings_dir);
    let initial_settings = state.settings();
    let task = SettingsActorTask {
        rx,
        event_bus,
        state,
        #[cfg(test)]
        set_last_connected_lv1_gate: None,
    };
    (SettingsHandle::new(tx), task, initial_settings)
}

async fn run_settings_actor(
    mut rx: mpsc::Receiver<SettingsCommand>,
    event_bus: AppEventBus,
    mut state: SettingsState,
    #[cfg(test)] mut set_last_connected_lv1_gate: Option<SetLastConnectedLv1Gate>,
) {
    while let Some(command) = rx.recv().await {
        handle_command(
            command,
            &event_bus,
            &mut state,
            #[cfg(test)]
            &mut set_last_connected_lv1_gate,
        )
        .await;
    }
    tracing::debug!(event = "settings_actor_stopped", "Settings actor stopped");
}

async fn handle_command(
    command: SettingsCommand,
    event_bus: &AppEventBus,
    state: &mut SettingsState,
    #[cfg(test)] set_last_connected_lv1_gate: &mut Option<SetLastConnectedLv1Gate>,
) {
    match command {
        SettingsCommand::GetSettings { reply } => {
            let _ = reply.send(state.settings());
        }
        SettingsCommand::ReplaceSettings { settings, reply } => {
            let result = match state.replace_settings(settings) {
                Ok(changed) => {
                    if changed {
                        let settings = state.settings();
                        log_settings_updated(&settings);
                        event_bus
                            .publish(AppEvent::Settings(SettingsEvent::StateChanged { settings }));
                    } else {
                        tracing::debug!(
                            event = "settings_update_noop",
                            "Settings already match requested values"
                        );
                    }
                    Ok(SettingsCommandResult { changed })
                }
                Err(error) => {
                    tracing::error!(
                        event = "settings_write_failed",
                        error = %error,
                        "Settings could not be saved"
                    );
                    Err(error)
                }
            };
            let _ = reply.send(result);
        }
        SettingsCommand::GetLastConnectedLv1 { reply } => {
            let _ = reply.send(state.last_connected_lv1());
        }
        SettingsCommand::SetLastConnectedLv1 {
            identity,
            runtime_generation,
            expected_generation,
            reply,
        } => {
            #[cfg(test)]
            if let Some(gate) = set_last_connected_lv1_gate.take() {
                let _ = gate.received.send(());
                let _ = gate.release.await;
            }
            let result = runtime_generation
                .if_current(expected_generation, || {
                    state.set_last_connected_lv1(identity).map(|_changed| ())
                })
                .await
                .unwrap_or(Ok(()));
            let _ = reply.send(result);
        }
    }
}

fn log_settings_updated(settings: &AppSettings) {
    tracing::info!(
        event = "settings_updated",
        auto_load_last_show_file = settings.auto_load_last_show_file,
        auto_save_sessions = settings.auto_save_sessions,
        time_display = time_display_label(&settings.time_display),
        fader_override_sensitivity = settings.fader_override_sensitivity,
        enable_extensive_diagnostics = settings.enable_extensive_diagnostics,
        same_scene_recall_enabled = settings.same_scene_recall_enabled,
        same_scene_recall_threshold_ms = settings.same_scene_recall_threshold_ms,
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
    use crate::connection_state::Lv1SystemIdentity;
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::settings::{AppSettings, SettingsEvent};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::oneshot;
    use tracing::field::{Field, Visit};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::{LookupSpan, Registry};

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct CapturedLogEvent {
        event: Option<String>,
        enable_extensive_diagnostics: Option<bool>,
        same_scene_recall_enabled: Option<bool>,
        same_scene_recall_threshold_ms: Option<String>,
    }

    #[derive(Clone, Default)]
    struct CapturedLogEvents(Arc<std::sync::Mutex<Vec<CapturedLogEvent>>>);

    impl<S> Layer<S> for CapturedLogEvents
    where
        S: tracing::Subscriber,
        S: for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = CapturedLogEvent::default();
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor);
        }
    }

    impl Visit for CapturedLogEvent {
        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "event" {
                self.event = Some(value.to_string());
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            match field.name() {
                "event" => {
                    self.event = Some(format!("{value:?}").trim_matches('"').to_string());
                }
                "enable_extensive_diagnostics" => {
                    self.enable_extensive_diagnostics = Some(format!("{value:?}") == "true");
                }
                "same_scene_recall_enabled" => {
                    self.same_scene_recall_enabled = Some(format!("{value:?}") == "true");
                }
                "same_scene_recall_threshold_ms" => {
                    self.same_scene_recall_threshold_ms = Some(format!("{value:?}"));
                }
                _ => {}
            }
        }
    }

    fn temp_settings_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("asc-settings-{name}-{unique}"))
    }

    fn identity(uuid: &str, host: &str, address: &str) -> Lv1SystemIdentity {
        Lv1SystemIdentity {
            uuid: Some(uuid.to_string()),
            host: Some(host.to_string()),
            address: address.to_string(),
            port: 50000,
        }
    }

    fn runtime_generation() -> crate::runtime::generation::RuntimeGeneration {
        crate::runtime::generation::RuntimeGeneration::default()
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
        let (handle, task, _initial_settings) = build_settings_actor(dir, event_bus);
        task.spawn();

        assert_eq!(get_settings(&handle).await, AppSettings::default());
    }

    #[tokio::test]
    async fn actor_loads_defaults_when_file_is_invalid() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("invalid");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), "not json").unwrap();
        let (handle, task, _initial_settings) = build_settings_actor(dir, event_bus);
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
        let (handle, task, _initial_settings) = build_settings_actor(dir, event_bus);
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
        let (handle, task, _initial_settings) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::ReplaceSettings {
                settings: AppSettings {
                    auto_save_sessions: true,
                    fader_override_sensitivity: 99,
                    same_scene_recall_threshold_ms: 9_999,
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
        assert!(saved.contains("\"sameSceneRecallThresholdMs\": 5000"));

        let received = events.recv().await.unwrap();
        assert!(matches!(
            received,
            AppEvent::Settings(SettingsEvent::StateChanged { settings })
                if settings.auto_save_sessions
                    && settings.fader_override_sensitivity == 10
                    && settings.same_scene_recall_threshold_ms == 5_000
        ));
    }

    #[tokio::test]
    async fn actor_does_not_save_or_publish_when_normalized_settings_are_unchanged() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let dir = temp_settings_dir("unchanged");
        let (handle, task, _initial_settings) = build_settings_actor(dir.clone(), event_bus);
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

    #[tokio::test]
    async fn actor_stores_and_returns_last_connected_lv1() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("connected-identity");
        let (handle, task, _) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();
        let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity: identity.clone(),
                runtime_generation: runtime_generation(),
                expected_generation: 0,
                reply,
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Ok(()));

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Some(identity));
        assert!(dir.join("settings.json").exists());
    }

    #[tokio::test]
    async fn actor_keeps_remembered_identity_private_across_mailbox_updates() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let dir = temp_settings_dir("remembered-identity-private");
        let (handle, task, _) = build_settings_actor(dir, event_bus);
        task.spawn();
        let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity: identity.clone(),
                runtime_generation: runtime_generation(),
                expected_generation: 0,
                reply,
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Ok(()));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), events.recv())
                .await
                .is_err()
        );

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::ReplaceSettings {
                settings: AppSettings {
                    auto_save_sessions: true,
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
        assert!(matches!(
            events.recv().await.unwrap(),
            AppEvent::Settings(SettingsEvent::StateChanged { settings }) if settings.auto_save_sessions
        ));

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Some(identity));
    }

    #[tokio::test]
    async fn actor_treats_stale_remembered_identity_update_as_successful_noop() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("stale-remembered-identity");
        let (handle, task, _) = build_settings_actor(dir, event_bus);
        task.spawn();
        let runtime_generation = crate::runtime::generation::RuntimeGeneration::default();
        runtime_generation.advance().await;
        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity: identity("uuid-new", "LV1-FOH", "192.168.1.36"),
                runtime_generation,
                expected_generation: 0,
                reply,
            })
            .await
            .expect("stale identity command should send");
        assert_eq!(
            rx.await.expect("stale identity reply should arrive"),
            Ok(())
        );

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), None);
    }

    #[tokio::test]
    async fn actor_preserves_remembered_identity_when_replacement_write_fails() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("failed-remembered-identity-write");
        let (handle, task, _) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();
        let runtime_generation = runtime_generation();
        let original = identity("uuid-old", "LV1-FOH", "192.168.1.35");

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity: original.clone(),
                runtime_generation: runtime_generation.clone(),
                expected_generation: 0,
                reply,
            })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Ok(()));

        std::fs::remove_file(dir.join("settings.json")).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        std::fs::write(&dir, "not a directory").unwrap();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity: identity("uuid-new", "LV1-FOH", "192.168.1.36"),
                runtime_generation,
                expected_generation: 0,
                reply,
            })
            .await
            .unwrap();
        assert!(rx.await.unwrap().is_err());

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Some(original));
    }

    #[tokio::test]
    async fn actor_logs_settings_update_fields() {
        let captured = CapturedLogEvents::default();
        let subscriber = Registry::default().with(captured.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        super::log_settings_updated(&AppSettings {
            enable_extensive_diagnostics: true,
            same_scene_recall_enabled: false,
            same_scene_recall_threshold_ms: 1_200,
            ..Default::default()
        });

        let events = captured.0.lock().unwrap();
        assert!(events.iter().any(|event| {
            event.event.as_deref() == Some("settings_updated")
                && event.enable_extensive_diagnostics == Some(true)
                && event.same_scene_recall_enabled == Some(false)
                && event.same_scene_recall_threshold_ms.as_deref() == Some("1200")
        }));
    }
}

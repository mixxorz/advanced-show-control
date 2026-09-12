use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::runtime::events::{AppEvent, AppEventBus};

use super::SettingsHandle;
use super::commands::{SettingsCommand, SettingsCommandResult};
use super::events::SettingsEvent;
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
    fn spawn_with_dispatch(self, dispatch: tracing::Dispatch) {
        use tracing::instrument::WithSubscriber;

        tauri::async_runtime::spawn(
            run_settings_actor(
                self.rx,
                self.event_bus,
                self.state,
                self.set_last_connected_lv1_gate,
            )
            .with_subscriber(dispatch),
        );
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

/// @cc [owner:mixxorz,label:privacy] initial-settings-projection-excludes-identity
/// Actor construction MUST seed retained settings state and its returned initial value from public
/// `AppSettings` only; the persisted remembered LV1 identity MUST remain available exclusively via
/// the dedicated settings commands and MUST NOT enter `SettingsEvent` projection data.
pub fn build_settings_actor(
    settings_dir: PathBuf,
    event_bus: AppEventBus,
) -> (SettingsHandle, SettingsActorTask, AppSettings) {
    let (tx, rx) = mpsc::channel(32);
    let state = SettingsState::load(settings_dir);
    let initial_settings = state.settings();
    event_bus.retain(&AppEvent::Settings(SettingsEvent::StateChanged {
        settings: initial_settings.clone(),
    }));
    let task = SettingsActorTask {
        rx,
        event_bus,
        state,
        #[cfg(test)]
        set_last_connected_lv1_gate: None,
    };
    (tx, task, initial_settings)
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

/**
 * @cc [owner:mixxorz,label:product] settings-replacement-observable-result
 * After a successful changed `ReplaceSettings`, the actor MUST publish
 * `SettingsEvent::StateChanged` and emit the `settings_updated` tracing event. Normalized no-ops
 * MUST report `changed: false` without either emission, and persistence failures MUST return the
 * underlying explanatory error without publishing or logging success.
 */
/**
 * @cc [owner:mixxorz,label:safety] remembered-identity-generation-gate
 * `SetLastConnectedLv1` MUST publish a changed identity only while `expected_generation` is current,
 * checking both before staging and atomically around publication. Stale work MUST return success as
 * a no-op, leave memory and `settings.json` unchanged, and clean up its unpublished staged file.
 */
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
            if runtime_generation.current().await != expected_generation {
                let _ = reply.send(Ok(()));
                return;
            }

            let result = match state.stage_last_connected_lv1(identity) {
                Ok(Some(staged)) => {
                    #[cfg(test)]
                    if let Some(gate) = set_last_connected_lv1_gate.take() {
                        let _ = gate.received.send(());
                        let _ = gate.release.await;
                    }
                    runtime_generation
                        .if_current(expected_generation, || state.publish_staged(staged))
                        .await
                        .unwrap_or(Ok(()))
                }
                Ok(None) => Ok(()),
                Err(error) => Err(error),
            };
            let _ = reply.send(result);
        }
    }
}

/// @cc [owner:mixxorz,label:privacy] settings-update-log-excludes-private-identity
/// Settings update logs MUST describe only projected public settings and MUST NOT include the
/// remembered LV1 identity or serialized settings document.
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
    use crate::test_support::TracingCapture;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::oneshot;
    use tracing_subscriber::prelude::*;

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

    async fn get_last_connected_lv1(handle: &SettingsHandle) -> Option<Lv1SystemIdentity> {
        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .unwrap();
        rx.await.unwrap()
    }

    fn staged_settings_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.file_name().is_some_and(|name| name != "settings.json"))
            .collect()
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
        std::fs::write(dir.join("settings.json"), r#"{"lastConnectedLv1":42}"#).unwrap();
        let (handle, task, _initial_settings) = build_settings_actor(dir, event_bus);
        task.spawn();

        assert_eq!(get_settings(&handle).await, AppSettings::default());
        assert_eq!(get_last_connected_lv1(&handle).await, None);
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
        let captured = TracingCapture::new();
        let dispatch =
            tracing::Dispatch::new(tracing_subscriber::registry().with(captured.clone()));
        task.spawn_with_dispatch(dispatch);

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
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(saved["autoSaveSessions"], true);
        assert_eq!(saved["faderOverrideSensitivity"], 10);
        assert_eq!(saved["sameSceneRecallThresholdMs"], 5_000);

        let received = events.recv().await.unwrap();
        assert!(matches!(
            received,
            AppEvent::Settings(SettingsEvent::StateChanged { settings })
                if settings.auto_save_sessions
                    && settings.fader_override_sensitivity == 10
                    && settings.same_scene_recall_threshold_ms == 5_000
        ));
        let logs = captured.matching("settings_updated", tracing::Level::INFO);
        assert!(logs.iter().any(|event| {
            event.fields.get("auto_save_sessions").map(String::as_str) == Some("true")
                && event
                    .fields
                    .get("fader_override_sensitivity")
                    .map(String::as_str)
                    == Some("10")
                && event
                    .fields
                    .get("same_scene_recall_threshold_ms")
                    .map(String::as_str)
                    == Some("5000")
        }));
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
    async fn actor_preserves_public_settings_when_publication_fails_and_cleans_staging() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("failed-publication");
        std::fs::create_dir_all(dir.join("settings.json")).unwrap();
        let (handle, task, _) = build_settings_actor(dir.clone(), event_bus);
        task.spawn();

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

        assert!(rx.await.unwrap().is_err());
        assert_eq!(get_settings(&handle).await, AppSettings::default());
        assert!(staged_settings_files(&dir).is_empty());
    }

    #[tokio::test]
    async fn actor_publishes_staged_last_connected_lv1_for_current_generation() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("connected-identity");
        let (handle, task, _) = build_settings_actor(dir.clone(), event_bus);
        let (staged_tx, staged_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        task.pause_set_last_connected_lv1(staged_tx, release_rx)
            .spawn();
        let identity = identity("uuid-1", "LV1-FOH", "192.168.1.35");
        let runtime_generation = runtime_generation();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity: identity.clone(),
                runtime_generation: runtime_generation.clone(),
                expected_generation: 0,
                reply,
            })
            .await
            .unwrap();
        staged_rx.await.expect("settings update should be staged");
        assert_eq!(staged_settings_files(&dir).len(), 1);
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                runtime_generation.current()
            )
            .await,
            Ok(0)
        );
        release_tx.send(()).unwrap();
        assert_eq!(rx.await.unwrap(), Ok(()));

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), Some(identity));
        let document: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(document["lastConnectedLv1"]["uuid"], "uuid-1");
        assert!(document.get("settings").is_none());
        assert!(staged_settings_files(&dir).is_empty());
    }

    #[tokio::test]
    async fn actor_keeps_remembered_identity_private_across_mailbox_updates() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let dir = temp_settings_dir("remembered-identity-private");
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
        assert_eq!(rx.await.unwrap(), Some(identity.clone()));

        let reloaded_bus = AppEventBus::default();
        let (reloaded, reloaded_task, reloaded_settings) = build_settings_actor(dir, reloaded_bus);
        reloaded_task.spawn();
        assert!(reloaded_settings.auto_save_sessions);
        assert_eq!(get_last_connected_lv1(&reloaded).await, Some(identity));
    }

    #[tokio::test]
    async fn actor_discards_staged_identity_when_generation_advances() {
        let event_bus = AppEventBus::default();
        let dir = temp_settings_dir("stale-remembered-identity");
        std::fs::create_dir_all(&dir).unwrap();
        let original_contents = r#"{
  "autoSaveSessions": true,
  "lastConnectedLv1": {
    "uuid": "uuid-old",
    "host": "LV1-FOH",
    "address": "192.168.1.35",
    "port": 50000
  }
}"#;
        std::fs::write(dir.join("settings.json"), original_contents).unwrap();
        let (handle, task, _) = build_settings_actor(dir.clone(), event_bus);
        let (staged_tx, staged_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        task.pause_set_last_connected_lv1(staged_tx, release_rx)
            .spawn();
        let runtime_generation = crate::runtime::generation::RuntimeGeneration::default();

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::SetLastConnectedLv1 {
                identity: identity("uuid-new", "LV1-FOH", "192.168.1.36"),
                runtime_generation: runtime_generation.clone(),
                expected_generation: 0,
                reply,
            })
            .await
            .expect("identity command should send");
        staged_rx.await.expect("settings update should be staged");
        assert_eq!(staged_settings_files(&dir).len(), 1);
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                runtime_generation.advance()
            )
            .await,
            Ok(1)
        );
        release_tx.send(()).unwrap();
        assert_eq!(
            rx.await.expect("stale identity reply should arrive"),
            Ok(())
        );

        let (reply, rx) = oneshot::channel();
        handle
            .send(SettingsCommand::GetLastConnectedLv1 { reply })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap(),
            Some(identity("uuid-old", "LV1-FOH", "192.168.1.35"))
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json")).unwrap(),
            original_contents
        );
        assert!(staged_settings_files(&dir).is_empty());
    }

    #[tokio::test]
    async fn actor_preserves_remembered_identity_when_staged_publication_fails() {
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
        std::fs::create_dir(dir.join("settings.json")).unwrap();

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
        assert!(staged_settings_files(&dir).is_empty());
    }
}

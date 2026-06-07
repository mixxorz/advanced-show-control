use std::collections::HashSet;

use lv1_scene_fade_utility::fade::curve::FadeCurve;
use lv1_scene_fade_utility::fade::types::{FadeConfig, FadeTarget};
use lv1_scene_fade_utility::lv1::model::{ConnectionStatus, SceneState};

use super::shell::{ShellState, scene_id};
use super::view::{LogSeverity, LogSource};

#[derive(Debug, Clone, PartialEq)]
pub struct SceneRecallFadeRequest {
    pub scene_id: String,
    pub scene_label: String,
    pub fade_config: FadeConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneRecallDecision {
    Start(SceneRecallFadeRequest),
    Skip,
    Blocked,
    StaleGeneration,
}

impl ShellState {
    pub async fn prepare_scene_recall_fade_for_generation(
        &self,
        generation: u64,
        recalled_scene: &SceneState,
    ) -> SceneRecallDecision {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return SceneRecallDecision::StaleGeneration;
        }

        let Some(snapshot) = inner.lv1_snapshot.as_mut() else {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: LV1 state is unavailable",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        };
        snapshot.scene = Some(recalled_scene.clone());
        let snapshot = snapshot.clone();

        if snapshot.connection != ConnectionStatus::Connected {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: LV1 is not connected",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let Some(current_scene) = snapshot.scene.as_ref() else {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: current scene snapshot is unavailable",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        };

        if current_scene.index != recalled_scene.index || current_scene.name != recalled_scene.name
        {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: scene identity mismatch",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let id = scene_id(recalled_scene.index, &recalled_scene.name);
        let Some(config) = inner
            .scene_configs
            .iter()
            .find(|config| config.scene_id == id)
            .cloned()
        else {
            return SceneRecallDecision::Skip;
        };

        if config.duration_ms == 0 {
            if inner.duration_zero_skip_logs.insert(id.clone()) {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Info,
                    format!(
                        "Auto fade skipped for scene {}: {}: duration is 0",
                        recalled_scene.index, recalled_scene.name
                    ),
                );
            }
            return SceneRecallDecision::Skip;
        }

        if inner.lockout {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: lockout is enabled",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        if snapshot.channels.is_empty() {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: live channel snapshot is empty",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let live_channels = snapshot
            .channels
            .iter()
            .map(|channel| (channel.group, channel.channel))
            .collect::<HashSet<_>>();
        let mut targets = Vec::with_capacity(config.scoped_channels.len());

        for scoped in &config.scoped_channels {
            if !live_channels.contains(&(scoped.group, scoped.channel)) {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    format!(
                        "Auto fade blocked for scene {}: {}: scoped channel group={} channel={} is missing from live topology",
                        recalled_scene.index, recalled_scene.name, scoped.group, scoped.channel
                    ),
                );
                return SceneRecallDecision::Blocked;
            }

            let Some(stored) = config
                .channel_configs
                .iter()
                .find(|entry| entry.group == scoped.group && entry.channel == scoped.channel)
            else {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    format!(
                        "Auto fade blocked for scene {}: {}: scoped channel group={} channel={} has no stored config",
                        recalled_scene.index, recalled_scene.name, scoped.group, scoped.channel
                    ),
                );
                return SceneRecallDecision::Blocked;
            };

            let Some(target_db) = stored.fader_db else {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Warning,
                    format!(
                        "Auto fade blocked for scene {}: {}: scoped channel group={} channel={} has no stored fader value",
                        recalled_scene.index, recalled_scene.name, scoped.group, scoped.channel
                    ),
                );
                return SceneRecallDecision::Blocked;
            };

            targets.push(FadeTarget {
                group: scoped.group,
                channel: scoped.channel,
                target_db,
            });
        }

        if targets.is_empty() {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: no scoped targets",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            return SceneRecallDecision::Blocked;
        }

        let scene_label = format!("{}: {}", recalled_scene.index, recalled_scene.name);
        inner.push_log(
            LogSource::App,
            LogSeverity::Info,
            format!(
                "Auto fade ready for scene {scene_label} with {} target{}",
                targets.len(),
                if targets.len() == 1 { "" } else { "s" }
            ),
        );

        SceneRecallDecision::Start(SceneRecallFadeRequest {
            scene_id: id,
            scene_label,
            fade_config: FadeConfig {
                targets,
                duration_ms: config.duration_ms,
                curve: FadeCurve::Linear,
            },
        })
    }

    pub async fn log_scene_recall_fader_info(&self, message: String) {
        let mut inner = self.inner.lock().await;
        inner.push_log(LogSource::App, LogSeverity::Info, message);
    }

    pub async fn log_scene_recall_fader_warning(&self, message: String) {
        let mut inner = self.inner.lock().await;
        inner.push_log(LogSource::App, LogSeverity::Warning, message);
    }
}

use std::collections::HashSet;
use advanced_show_control::fade::types::FadeConfig;
use advanced_show_control::lv1::types::{Lv1StateSnapshot, SceneState};
use advanced_show_control::scene_recall::policy::{
    decide_scene_recall, RecallPolicyDecision, RecallPolicyInput,
};
use advanced_show_control::show::types::ShowSnapshot;

use super::shell::{ShellInner, ShellState, scene_id};
use super::view::{LogSeverity, LogSource};

#[derive(Debug, Default)]
pub(super) struct SceneRecallLogState {
    duration_zero_skip_log_generation: u64,
    duration_zero_skip_log_scene_ids: HashSet<String>,
}

impl SceneRecallLogState {
    pub(super) fn clear_for_generation(&mut self, generation: u64) {
        self.duration_zero_skip_log_generation = generation;
        self.duration_zero_skip_log_scene_ids.clear();
    }

    pub(super) fn should_log_duration_zero_skip(&mut self, generation: u64, scene_id: &str) -> bool {
        if self.duration_zero_skip_log_generation != generation {
            self.clear_for_generation(generation);
        }
        self.duration_zero_skip_log_scene_ids.insert(scene_id.to_string())
    }
}

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
    #[cfg(test)]
    pub async fn prepare_scene_recall_fade_for_generation(
        &self,
        generation: u64,
        recalled_scene: &SceneState,
    ) -> SceneRecallDecision {
        let mut inner = self.inner.lock().await;
        let show = self
            .show
            .get_snapshot()
            .await
            .unwrap_or(ShowSnapshot { lockout: false, scene_configs: Vec::new() });
        let mut log_state = self.scene_recall_logs.lock().await;
        prepare_scene_recall_fade_locked(&mut inner, &mut log_state, generation, recalled_scene, show, true)
    }

    pub async fn prepare_scene_recall_fade_with_lv1_snapshot_for_generation(
        &self,
        generation: u64,
        recalled_scene: &SceneState,
        snapshot: Lv1StateSnapshot,
    ) -> SceneRecallDecision {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return SceneRecallDecision::StaleGeneration;
        }

        inner.lv1_snapshot = Some(snapshot);
        let show = self
            .show
            .get_snapshot()
            .await
            .unwrap_or(ShowSnapshot { lockout: false, scene_configs: Vec::new() });
        let mut log_state = self.scene_recall_logs.lock().await;
        prepare_scene_recall_fade_locked(&mut inner, &mut log_state, generation, recalled_scene, show, false)
    }

    pub async fn is_generation_current(&self, generation: u64) -> bool {
        self.inner.lock().await.generation == generation
    }

    pub async fn log_scene_recall_fader_info_for_generation(
        &self,
        generation: u64,
        message: String,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return false;
        }

        inner.push_log(LogSource::App, LogSeverity::Info, message);
        true
    }

    pub async fn log_scene_recall_fader_warning_for_generation(
        &self,
        generation: u64,
        message: String,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation {
            return false;
        }

        inner.push_log(LogSource::App, LogSeverity::Warning, message);
        true
    }
}

fn prepare_scene_recall_fade_locked(
    inner: &mut ShellInner,
    log_state: &mut SceneRecallLogState,
    generation: u64,
    recalled_scene: &SceneState,
    show: ShowSnapshot,
    overwrite_snapshot_scene: bool,
) -> SceneRecallDecision {
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
    let id = scene_id(recalled_scene.index, &recalled_scene.name);
    if overwrite_snapshot_scene {
        snapshot.scene = Some(recalled_scene.clone());
    }
    let snapshot = snapshot.clone();
    let scene_config = show.scene_configs.iter().find(|config| config.scene_id == id).cloned();

    let decision = decide_scene_recall(RecallPolicyInput {
        recalled_scene: recalled_scene.clone(),
        lv1_snapshot: snapshot,
        lockout: show.lockout,
        scene_config,
    });

    match decision {
        RecallPolicyDecision::Start(fade_config) => {
            let scene_label = format!("{}: {}", recalled_scene.index, recalled_scene.name);
            inner.push_log(
                LogSource::App,
                LogSeverity::Info,
                format!(
                    "Auto fade ready for scene {scene_label} with {} target{}",
                    fade_config.targets.len(),
                    if fade_config.targets.len() == 1 { "" } else { "s" }
                ),
            );
            SceneRecallDecision::Start(SceneRecallFadeRequest { scene_id: id, scene_label, fade_config })
        }
        RecallPolicyDecision::Skip { reason } => {
            if reason == "duration is 0" && log_state.should_log_duration_zero_skip(generation, &id) {
                inner.push_log(
                    LogSource::App,
                    LogSeverity::Info,
                    format!(
                        "Auto fade skipped for scene {}: {}: duration is 0",
                        recalled_scene.index, recalled_scene.name
                    ),
                );
            }
            SceneRecallDecision::Skip
        }
        RecallPolicyDecision::Blocked { reason } => {
            inner.push_log(
                LogSource::App,
                LogSeverity::Warning,
                format!(
                    "Auto fade blocked for scene {}: {}: {reason}",
                    recalled_scene.index, recalled_scene.name
                ),
            );
            SceneRecallDecision::Blocked
        }
    }
}

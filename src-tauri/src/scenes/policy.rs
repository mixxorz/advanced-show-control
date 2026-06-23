use crate::fade::{FadeConfig, FadeCurve, FadeParameter, FadeSceneIdentity, FadeTarget};
use crate::lv1::{ConnectionStatus, Lv1StateSnapshot, PanMode, SceneState};
use crate::show::SceneConfig;

pub struct RecallPolicyInput {
    pub recalled_scene: SceneState,
    pub lv1_snapshot: Lv1StateSnapshot,
    pub lockout: bool,
    pub scene_config: Option<SceneConfig>,
}

#[derive(Debug)]
pub enum RecallPolicyDecision {
    Start(FadeConfig),
    Skip { reason: String },
    Blocked { reason: String },
}

pub fn decide_scene_recall(input: RecallPolicyInput) -> RecallPolicyDecision {
    let RecallPolicyInput {
        recalled_scene,
        lv1_snapshot,
        lockout,
        scene_config,
    } = input;

    if lockout {
        return blocked("lockout is enabled");
    }
    if lv1_snapshot.connection != ConnectionStatus::Connected {
        return blocked("LV1 is not connected");
    }
    let Some(current_scene) = lv1_snapshot.scene.as_ref() else {
        return blocked("current scene snapshot is unavailable");
    };
    if current_scene.index != recalled_scene.index || current_scene.name != recalled_scene.name {
        return blocked("scene identity mismatch");
    }

    let Some(config) = scene_config else {
        return skipped("scene config is missing");
    };
    if !config.scope_toggles.faders && !config.scope_toggles.pan {
        return skipped("no applicable targets");
    }
    if lv1_snapshot.channels.is_empty() {
        return blocked("live channel snapshot is empty");
    }

    let live_channels = lv1_snapshot
        .channels
        .iter()
        .map(|c| (c.group, c.channel))
        .collect::<std::collections::HashSet<_>>();
    let mut targets = Vec::with_capacity(config.scoped_channels.len());
    let pan_enabled = config.scope_toggles.pan;
    let faders_enabled = config.scope_toggles.faders;

    for scoped in &config.scoped_channels {
        if !live_channels.contains(&(scoped.group, scoped.channel)) {
            return blocked(format!(
                "scoped channel group={} channel={} is missing from live topology",
                scoped.group, scoped.channel
            ));
        }
        let Some(stored) = config
            .channel_configs
            .iter()
            .find(|entry| entry.group == scoped.group && entry.channel == scoped.channel)
        else {
            return blocked(format!(
                "scoped channel group={} channel={} has no stored config",
                scoped.group, scoped.channel
            ));
        };
        if faders_enabled {
            let Some(target_db) = stored.fader_db else {
                return blocked(format!(
                    "scoped channel group={} channel={} has no stored fader value",
                    scoped.group, scoped.channel
                ));
            };
            targets.push(FadeTarget {
                group: scoped.group,
                channel: scoped.channel,
                parameter: FadeParameter::FaderDb,
                target: target_db,
            });
        }
        if pan_enabled {
            match stored.pan_mode.as_ref() {
                Some(PanMode::None) | None => {}
                Some(PanMode::Mono) => {
                    if let Some(pan) = stored.pan {
                        targets.push(FadeTarget {
                            group: scoped.group,
                            channel: scoped.channel,
                            parameter: FadeParameter::Pan,
                            target: pan,
                        });
                    }
                }
                Some(PanMode::Stereo) => {
                    if let Some(pan) = stored.pan {
                        targets.push(FadeTarget {
                            group: scoped.group,
                            channel: scoped.channel,
                            parameter: FadeParameter::Pan,
                            target: pan,
                        });
                    }
                    if let Some(balance) = stored.balance {
                        targets.push(FadeTarget {
                            group: scoped.group,
                            channel: scoped.channel,
                            parameter: FadeParameter::Balance,
                            target: balance,
                        });
                    }
                    if let Some(width) = stored.width {
                        targets.push(FadeTarget {
                            group: scoped.group,
                            channel: scoped.channel,
                            parameter: FadeParameter::Width,
                            target: width,
                        });
                    }
                }
            }
        }
    }

    if targets.is_empty() {
        return skipped("no applicable targets");
    }
    RecallPolicyDecision::Start(FadeConfig {
        scene: FadeSceneIdentity {
            index: recalled_scene.index,
            name: recalled_scene.name,
        },
        targets,
        duration_ms: config.duration_ms,
        curve: FadeCurve::Linear,
    })
}

fn skipped(reason: impl Into<String>) -> RecallPolicyDecision {
    RecallPolicyDecision::Skip {
        reason: reason.into(),
    }
}
fn blocked(reason: impl Into<String>) -> RecallPolicyDecision {
    RecallPolicyDecision::Blocked {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, PanMode, SceneState};
    use crate::show::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles};

    fn snapshot(scene: Option<SceneState>, channels: Vec<ChannelInfo>) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene,
            scene_list: Vec::new(),
            channels,
        }
    }

    fn config(
        duration_ms: u64,
        fader_db: Option<f64>,
        pan: Option<f64>,
        balance: Option<f64>,
        width: Option<f64>,
        pan_mode: Option<PanMode>,
    ) -> SceneConfig {
        SceneConfig {
            internal_scene_id: uuid::Uuid::from_u128(0x11111111111141118111111111111111),
            scene_index: Some(1),
            scene_name: "Intro".to_string(),
            duration_ms,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db,
                pan,
                balance,
                width,
                pan_mode,
            }],
            scoped_channels: vec![ChannelRef {
                group: 0,
                channel: 2,
            }],
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    #[test]
    fn blocks_when_lockout_enabled() {
        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: true,
            scene_config: Some(config(1000, Some(-12.5), None, None, None, None)),
        });
        assert!(matches!(decision, RecallPolicyDecision::Blocked { .. }));
    }

    #[test]
    fn blocks_when_scene_identity_mismatches_snapshot() {
        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 2,
                    name: "Verse".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: false,
            scene_config: Some(config(1000, Some(-12.5), None, None, None, None)),
        });
        assert!(matches!(decision, RecallPolicyDecision::Blocked { .. }));
    }

    #[test]
    fn skips_when_scene_config_is_missing() {
        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: false,
            scene_config: None,
        });
        assert!(matches!(decision, RecallPolicyDecision::Skip { .. }));
    }

    #[test]
    fn skips_when_both_scope_toggles_are_disabled() {
        let mut scene_config = config(
            1000,
            Some(-12.5),
            Some(0.25),
            Some(-0.5),
            Some(1.0),
            Some(PanMode::Stereo),
        );
        scene_config.scope_toggles.faders = false;
        scene_config.scope_toggles.pan = false;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: Some(0.0),
                    balance: Some(0.0),
                    width: Some(0.0),
                    pan_mode: Some(PanMode::Stereo),
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        assert!(matches!(
            decision,
            RecallPolicyDecision::Skip { reason } if reason == "no applicable targets"
        ));
    }

    #[test]
    fn pan_only_mono_builds_pan_target() {
        let mut scene_config = config(1000, None, Some(0.25), None, None, Some(PanMode::Mono));
        scene_config.scope_toggles.faders = false;
        scene_config.scope_toggles.pan = true;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: Some(0.0),
                    balance: None,
                    width: None,
                    pan_mode: Some(PanMode::Mono),
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        match decision {
            RecallPolicyDecision::Start(fade) => {
                assert_eq!(fade.targets.len(), 1);
                assert_eq!(fade.targets[0].parameter, FadeParameter::Pan);
                assert_eq!(fade.targets[0].target, 0.25);
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn pan_only_stereo_builds_pan_family_targets() {
        let mut scene_config = config(
            1000,
            None,
            Some(0.25),
            Some(-0.5),
            Some(1.0),
            Some(PanMode::Stereo),
        );
        scene_config.scope_toggles.faders = false;
        scene_config.scope_toggles.pan = true;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: Some(0.0),
                    balance: Some(0.0),
                    width: Some(0.0),
                    pan_mode: Some(PanMode::Stereo),
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        match decision {
            RecallPolicyDecision::Start(fade) => {
                assert_eq!(fade.targets.len(), 3);
                assert!(
                    fade.targets
                        .iter()
                        .any(|t| t.parameter == FadeParameter::Pan && t.target == 0.25)
                );
                assert!(
                    fade.targets
                        .iter()
                        .any(|t| t.parameter == FadeParameter::Balance && t.target == -0.5)
                );
                assert!(
                    fade.targets
                        .iter()
                        .any(|t| t.parameter == FadeParameter::Width && t.target == 1.0)
                );
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn pan_only_stereo_missing_balance_uses_available_targets() {
        let mut scene_config = config(
            1000,
            None,
            Some(0.25),
            None,
            Some(1.0),
            Some(PanMode::Stereo),
        );
        scene_config.scope_toggles.faders = false;
        scene_config.scope_toggles.pan = true;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: Some(0.0),
                    balance: Some(0.0),
                    width: Some(0.0),
                    pan_mode: Some(PanMode::Stereo),
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        let RecallPolicyDecision::Start(fade) = decision else {
            panic!("unexpected decision: {decision:?}");
        };
        assert_eq!(fade.targets.len(), 2);
        assert!(fade.targets.iter().any(|target| {
            target.group == 0
                && target.channel == 2
                && target.parameter == FadeParameter::Pan
                && target.target == 0.25
        }));
        assert!(fade.targets.iter().any(|target| {
            target.group == 0
                && target.channel == 2
                && target.parameter == FadeParameter::Width
                && target.target == 1.0
        }));
        assert!(
            !fade
                .targets
                .iter()
                .any(|target| target.parameter == FadeParameter::Balance)
        );
    }

    #[test]
    fn pan_only_stereo_missing_width_uses_available_targets() {
        let mut scene_config = config(
            1000,
            None,
            Some(0.25),
            Some(-0.5),
            None,
            Some(PanMode::Stereo),
        );
        scene_config.scope_toggles.faders = false;
        scene_config.scope_toggles.pan = true;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: Some(0.0),
                    balance: Some(0.0),
                    width: Some(0.0),
                    pan_mode: Some(PanMode::Stereo),
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        let RecallPolicyDecision::Start(fade) = decision else {
            panic!("unexpected decision: {decision:?}");
        };
        assert_eq!(fade.targets.len(), 2);
        assert!(fade.targets.iter().any(|target| {
            target.group == 0
                && target.channel == 2
                && target.parameter == FadeParameter::Pan
                && target.target == 0.25
        }));
        assert!(fade.targets.iter().any(|target| {
            target.group == 0
                && target.channel == 2
                && target.parameter == FadeParameter::Balance
                && target.target == -0.5
        }));
        assert!(
            !fade
                .targets
                .iter()
                .any(|target| target.parameter == FadeParameter::Width)
        );
    }

    #[test]
    fn pan_only_no_pan_mode_skips_without_blocking() {
        let mut scene_config = config(1000, None, None, None, None, Some(PanMode::None));
        scene_config.scope_toggles.faders = false;
        scene_config.scope_toggles.pan = true;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: Some(PanMode::None),
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        assert!(
            matches!(decision, RecallPolicyDecision::Skip { reason } if reason == "no applicable targets")
        );
    }

    #[test]
    fn fader_scope_still_blocks_missing_fader_value() {
        let mut scene_config = config(1000, None, None, None, None, None);
        scene_config.scope_toggles.faders = true;
        scene_config.scope_toggles.pan = false;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        assert!(
            matches!(decision, RecallPolicyDecision::Blocked { reason } if reason == "scoped channel group=0 channel=2 has no stored fader value")
        );
    }

    #[test]
    fn both_scopes_on_include_fader_and_pan_family_targets() {
        let mut scene_config = config(
            1000,
            Some(-12.5),
            Some(0.25),
            Some(-0.5),
            Some(1.0),
            Some(PanMode::Stereo),
        );
        scene_config.scope_toggles.faders = true;
        scene_config.scope_toggles.pan = true;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: Some(0.0),
                    balance: Some(0.0),
                    width: Some(0.0),
                    pan_mode: Some(PanMode::Stereo),
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        match decision {
            RecallPolicyDecision::Start(fade) => {
                assert_eq!(fade.targets.len(), 4);
                assert!(
                    fade.targets
                        .iter()
                        .any(|t| t.parameter == FadeParameter::FaderDb && t.target == -12.5)
                );
                assert!(
                    fade.targets
                        .iter()
                        .any(|t| t.parameter == FadeParameter::Pan && t.target == 0.25)
                );
                assert!(
                    fade.targets
                        .iter()
                        .any(|t| t.parameter == FadeParameter::Balance && t.target == -0.5)
                );
                assert!(
                    fade.targets
                        .iter()
                        .any(|t| t.parameter == FadeParameter::Width && t.target == 1.0)
                );
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn starts_when_scene_config_and_live_topology_are_valid() {
        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: false,
            scene_config: Some(config(4000, Some(-12.5), None, None, None, None)),
        });
        match decision {
            RecallPolicyDecision::Start(fade) => {
                assert_eq!(fade.duration_ms, 4000);
                assert_eq!(fade.targets.len(), 1);
                assert_eq!(fade.targets[0].target, -12.5);
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn starts_zero_duration_scene_with_intact_target_data() {
        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: false,
            scene_config: Some(config(0, Some(-12.5), None, None, None, None)),
        });
        assert!(matches!(
            decision,
            RecallPolicyDecision::Start(config)
                if config.duration_ms == 0
                    && config.targets.len() == 1
                    && config.targets[0].group == 0
                    && config.targets[0].channel == 2
                    && config.targets[0].target == -12.5
        ));
    }

    #[test]
    fn skips_when_fader_scope_is_disabled() {
        let mut scene_config = config(1000, Some(-12.5), None, None, None, None);
        scene_config.scope_toggles.faders = false;

        let decision = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: false,
            scene_config: Some(scene_config),
        });

        assert!(matches!(
            decision,
            RecallPolicyDecision::Skip { reason } if reason == "no applicable targets"
        ));
    }

    #[test]
    fn missing_topology_and_stored_value_cases_block() {
        let missing_topology = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                Vec::new(),
            ),
            lockout: false,
            scene_config: Some(config(1000, Some(-12.5), None, None, None, None)),
        });
        assert!(matches!(
            missing_topology,
            RecallPolicyDecision::Blocked { .. }
        ));
        let missing_value = decide_scene_recall(RecallPolicyInput {
            recalled_scene: SceneState {
                index: 1,
                name: "Intro".to_string(),
            },
            lv1_snapshot: snapshot(
                Some(SceneState {
                    index: 1,
                    name: "Intro".to_string(),
                }),
                vec![ChannelInfo {
                    group: 0,
                    channel: 2,
                    name: "Ch 2".to_string(),
                    gain_db: 0.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
            ),
            lockout: false,
            scene_config: Some(config(1000, None, None, None, None, None)),
        });
        assert!(matches!(
            missing_value,
            RecallPolicyDecision::Blocked { .. }
        ));
    }
}

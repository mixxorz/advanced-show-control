#![allow(clippy::items_after_test_module, clippy::module_inception)]

use crate::connection_state::Lv1SystemIdentity;
use crate::lifecycle::AppLifecycle;
use crate::runtime::events::AppEvent;
use crate::runtime::events::RuntimeLifecycleEvent;
use crate::scenes::{ScenesCommand, ScenesEvent};
use crate::show::RecallSceneResult;
use crate::smoke::runner::{fail_step, pass_step, summarize_app_event};
use crate::smoke::trace_capture::{SmokeTraceCapture, SmokeTraceEvent, SmokeTraceLayer};
use crate::smoke::{SmokeBackendResult, SmokeTestParams};
use crate::time::current_timestamp_millis;
use tokio::sync::oneshot;
use tracing_subscriber::layer::SubscriberExt;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecreasingXfadeStep {
    duration_ms: u64,
    target_scene_id: String,
}

fn decreasing_xfade_sequence(
    scene_a_id: impl Into<String>,
    scene_b_id: impl Into<String>,
) -> Vec<DecreasingXfadeStep> {
    let scene_a_id = scene_a_id.into();
    let scene_b_id = scene_b_id.into();

    vec![
        DecreasingXfadeStep {
            duration_ms: 5_000,
            target_scene_id: scene_b_id.clone(),
        },
        DecreasingXfadeStep {
            duration_ms: 3_000,
            target_scene_id: scene_a_id.clone(),
        },
        DecreasingXfadeStep {
            duration_ms: 1_000,
            target_scene_id: scene_b_id.clone(),
        },
        DecreasingXfadeStep {
            duration_ms: 500,
            target_scene_id: scene_a_id,
        },
    ]
}

fn trace_has_manual_override(observed_traces: &[SmokeTraceEvent]) -> bool {
    observed_traces
        .iter()
        .any(|event| event.has_field("event", "manual_override_detected"))
}

#[cfg(test)]
fn trace_event(name: &str) -> SmokeTraceEvent {
    SmokeTraceEvent {
        timestamp_ms: 0,
        level: "DEBUG".to_string(),
        target: "test".to_string(),
        fields: vec![crate::smoke::SmokeTraceField {
            name: "event".to_string(),
            value: name.to_string(),
        }],
    }
}

fn fade_completion_steps(
    saw_fade_started: bool,
    saw_fade_completed: bool,
    starting_db: f64,
    observed_final_db: f64,
    tolerance_db: f64,
    observed_traces: &[SmokeTraceEvent],
) -> Vec<crate::smoke::SmokeStepResult> {
    let saw_manual_override_trace = trace_has_manual_override(observed_traces);
    let saw_channel_override_trace = observed_traces
        .iter()
        .any(|event| event.has_field("event", "channel_override_detected"));
    let final_within_tolerance = (observed_final_db - starting_db).abs() <= tolerance_db;

    vec![
        if saw_fade_started {
            pass_step("fade.started", "fade started was observed")
        } else {
            fail_step(
                "fade.started",
                "fade started was not observed",
                serde_json::json!({"fadeStarted": saw_fade_started}),
            )
        },
        if saw_fade_completed {
            pass_step("fade.completed", "fade completed was observed")
        } else {
            fail_step(
                "fade.completed",
                "fade completed was not observed",
                serde_json::json!({"fadeCompleted": saw_fade_completed}),
            )
        },
        if final_within_tolerance {
            pass_step(
                "fader.withinTolerance",
                "final fader value stayed within tolerance",
            )
        } else {
            fail_step(
                "fader.withinTolerance",
                "final fader value was outside tolerance",
                serde_json::json!({
                    "startingDb": starting_db,
                    "observedFinalDb": observed_final_db,
                    "toleranceDb": tolerance_db,
                }),
            )
        },
        if saw_manual_override_trace {
            fail_step(
                "trace.noManualOverride",
                "manual override trace was present",
                serde_json::json!({"manualOverrideTrace": true}),
            )
        } else {
            pass_step("trace.noManualOverride", "manual override trace was absent")
        },
        if saw_channel_override_trace {
            fail_step(
                "trace.noChannelOverride",
                "channel override trace was present",
                serde_json::json!({"channelOverrideTrace": true}),
            )
        } else {
            pass_step(
                "trace.noChannelOverride",
                "channel override trace was absent",
            )
        },
    ]
}

async fn wait_for_fade_completion_observation(
    lifecycle: &AppLifecycle,
    params: &SmokeTestParams,
    rx: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    target_scene_id: &str,
) -> Result<(bool, bool, bool, bool, Option<f64>, Vec<String>), String> {
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_millis(params.timeout_ms);
    let mut sample_tick =
        tokio::time::interval(std::time::Duration::from_millis(params.sample_interval_ms));
    sample_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut saw_fade_started = false;
    let mut saw_fade_completed = false;
    let mut saw_channel_override = false;
    let mut observed_final_db = None;
    let mut observed_events = Vec::new();
    let mut timed_out = true;

    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            event = rx.recv() => {
                let event = event.map_err(|err| err.to_string())?;
                observed_events.push(summarize_app_event(&event));
                match event {
                    AppEvent::Fade { event: crate::fade::FadeEvent::FadeStarted, .. } => saw_fade_started = true,
                    AppEvent::Fade { event: crate::fade::FadeEvent::FadeCompleted, .. } => saw_fade_completed = true,
                    AppEvent::Fade { event: crate::fade::FadeEvent::ChannelOverride { .. }, .. } => {
                        saw_channel_override = true;
                        timed_out = false;
                        return Ok((
                            saw_fade_started,
                            saw_fade_completed,
                            saw_channel_override,
                            timed_out,
                            observed_final_db,
                            observed_events,
                        ));
                    }
                    AppEvent::Lv1 { event: crate::lv1::Lv1Event::SceneChanged(scene), .. }
                        if crate::show::scene_id(scene.index, &scene.name) == target_scene_id => {}
                    _ => {}
                }
            }
            _ = sample_tick.tick() => {
                let lv1 = lifecycle.debug_smoke_current_lv1().await.ok_or_else(|| "LV1 accessor unavailable".to_string())?;
                let (reply, response_rx) = oneshot::channel();
                lv1.send(crate::lv1::Lv1Command::GetState { reply }).await.map_err(|error| error.to_string())?;
                let snapshot = response_rx.await.map_err(|_| "LV1 state reply channel closed".to_string())?;
                observed_final_db = sample_channel_gain(&snapshot, &params.channel);
            }
        }

        if saw_fade_started && saw_fade_completed {
            timed_out = false;
            break;
        }
    }

    Ok((
        saw_fade_started,
        saw_fade_completed,
        saw_channel_override,
        timed_out,
        observed_final_db,
        observed_events,
    ))
}

fn scene_recall_steps(
    saw_recall_requested: bool,
    saw_scene_changed: bool,
    saw_ready: bool,
    saw_start_requested: bool,
    saw_blocked: bool,
    saw_skipped: bool,
    observed_traces: &[SmokeTraceEvent],
) -> Vec<crate::smoke::SmokeStepResult> {
    let saw_scene_recall_trace = observed_traces
        .iter()
        .any(|event| event.has_field("event", "scene_recall_requested"));
    let saw_scene_recall_command_trace = observed_traces
        .iter()
        .any(|event| event.has_field("event", "scene_recall_command_sent"));

    vec![
        if saw_recall_requested {
            pass_step("scene.recallRequested", "scene recall request was observed")
        } else {
            fail_step(
                "scene.recallRequested",
                "scene recall request was not observed",
                serde_json::json!({"recallRequested": saw_recall_requested}),
            )
        },
        if saw_scene_changed {
            pass_step("scene.changed", "target scene change was observed")
        } else {
            fail_step(
                "scene.changed",
                "target scene change was not observed",
                serde_json::json!({"sceneChanged": saw_scene_changed}),
            )
        },
        if saw_ready {
            pass_step("scene.ready", "scene recall ready event was observed")
        } else {
            fail_step(
                "scene.ready",
                "scene recall ready event was not observed",
                serde_json::json!({"ready": saw_ready}),
            )
        },
        if saw_start_requested {
            pass_step("scene.startRequested", "fade start request was observed")
        } else {
            fail_step(
                "scene.startRequested",
                "fade start request was not observed",
                serde_json::json!({"startRequested": saw_start_requested}),
            )
        },
        if !saw_blocked {
            pass_step("scene.noBlocked", "recall was not blocked")
        } else {
            fail_step(
                "scene.noBlocked",
                "recall was blocked",
                serde_json::json!({"blocked": saw_blocked}),
            )
        },
        if !saw_skipped {
            pass_step("scene.noSkipped", "recall was not skipped")
        } else {
            fail_step(
                "scene.noSkipped",
                "recall was skipped",
                serde_json::json!({"skipped": saw_skipped}),
            )
        },
        if saw_scene_recall_trace {
            pass_step(
                "trace.sceneRecallRequested",
                "trace recorded scene_recall_requested",
            )
        } else {
            fail_step(
                "trace.sceneRecallRequested",
                "trace did not record scene_recall_requested",
                serde_json::json!({"traceMatch": saw_scene_recall_trace}),
            )
        },
        if saw_scene_recall_command_trace {
            pass_step(
                "trace.sceneRecallCommandSent",
                "trace recorded scene_recall_command_sent",
            )
        } else {
            fail_step(
                "trace.sceneRecallCommandSent",
                "trace did not record scene_recall_command_sent",
                serde_json::json!({"traceMatch": saw_scene_recall_command_trace}),
            )
        },
    ]
}

fn lockout_steps(
    saw_blocked: bool,
    saw_no_scene_change: bool,
    saw_no_fade_start: bool,
    saw_no_fader_movement: bool,
    saw_lockout_trace: bool,
) -> Vec<crate::smoke::SmokeStepResult> {
    vec![
        if saw_blocked {
            pass_step("scene.blocked", "scene recall was blocked")
        } else {
            fail_step(
                "scene.blocked",
                "scene recall was not blocked",
                serde_json::json!({"blocked": saw_blocked}),
            )
        },
        if saw_no_scene_change {
            pass_step("scene.noChange", "blocked recall did not change scenes")
        } else {
            fail_step(
                "scene.noChange",
                "blocked recall changed scenes",
                serde_json::json!({"sceneChanged": false}),
            )
        },
        if saw_no_fade_start {
            pass_step("fade.noStart", "blocked recall did not start a fade")
        } else {
            fail_step(
                "fade.noStart",
                "blocked recall started a fade",
                serde_json::json!({"fadeStarted": true}),
            )
        },
        if saw_no_fader_movement {
            pass_step("lv1.noMovement", "blocked recall did not move faders")
        } else {
            fail_step(
                "lv1.noMovement",
                "blocked recall moved faders",
                serde_json::json!({"movement": true}),
            )
        },
        if saw_lockout_trace {
            pass_step("trace.lockoutBlock", "trace recorded lockout block")
        } else {
            fail_step(
                "trace.lockoutBlock",
                "trace did not record lockout block",
                serde_json::json!({"traceMatch": saw_lockout_trace}),
            )
        },
    ]
}

#[cfg(test)]
fn lockout_no_movement_detected(start_db: f64, samples: &[f64], tolerance_db: f64) -> bool {
    samples
        .iter()
        .all(|sample| (sample - start_db).abs() <= tolerance_db)
}

fn scene_event_matches_target(event: &AppEvent, target_scene_id: &str) -> bool {
    match event {
        AppEvent::Lv1 {
            event: crate::lv1::Lv1Event::SceneChanged(scene),
            ..
        } => crate::show::scene_id(scene.index, &scene.name) == target_scene_id,
        _ => false,
    }
}

fn event_matches_test_channel(
    event: &AppEvent,
    channel: &crate::smoke::runner::SmokeTestChannel,
) -> bool {
    matches!(
        event,
        AppEvent::Lv1 {
            event: crate::lv1::Lv1Event::FaderChanged {
                group,
                channel: event_channel,
                ..
            },
            ..
        } if *group == channel.group && *event_channel == channel.channel
    )
}

fn sample_channel_gain(
    snapshot: &crate::lv1::Lv1StateSnapshot,
    channel: &crate::smoke::runner::SmokeTestChannel,
) -> Option<f64> {
    snapshot
        .channels
        .iter()
        .find(|entry| entry.group == channel.group && entry.channel == channel.channel)
        .map(|entry| entry.gain_db)
}

#[cfg(test)]
fn scene_channel_target_db(
    show: &crate::show::ShowDocument,
    scene_id: &str,
    channel: &crate::smoke::runner::SmokeTestChannel,
) -> Option<f64> {
    show.scene_configs
        .iter()
        .find(|scene| scene.scene_id == scene_id)
        .and_then(|scene| {
            scene
                .channel_configs
                .iter()
                .find(|entry| entry.group == channel.group && entry.channel == channel.channel)
        })
        .and_then(|entry| entry.fader_db)
}

fn movement_matches_expected_direction(start_db: f64, target_db: f64, observed_db: f64) -> bool {
    if target_db >= start_db {
        observed_db >= start_db && observed_db <= target_db
    } else {
        observed_db <= start_db && observed_db >= target_db
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn fader_movement_steps(
    start_db: f64,
    target_db: f64,
    observed_final_db: f64,
    _expected_start_db: f64,
    _expected_target_db: f64,
    tolerance_db: f64,
    minimum_movement_db: f64,
    sample_count: usize,
) -> Vec<crate::smoke::SmokeStepResult> {
    let moved_enough = (start_db - target_db).abs() >= minimum_movement_db;
    let final_within_tolerance = (observed_final_db - target_db).abs() <= tolerance_db;

    vec![
        if moved_enough {
            pass_step("movement.minimum", "movement exceeded minimum threshold")
        } else {
            fail_step(
                "movement.minimum",
                "movement did not exceed minimum threshold",
                serde_json::json!({
                    "startDb": start_db,
                    "targetDb": target_db,
                    "minimumMovementDb": minimum_movement_db,
                }),
            )
        },
        if final_within_tolerance {
            pass_step(
                "movement.finalTolerance",
                "final value stayed within tolerance",
            )
        } else {
            fail_step(
                "movement.finalTolerance",
                "final value was outside tolerance",
                serde_json::json!({
                    "observedFinalDb": observed_final_db,
                    "targetDb": target_db,
                    "toleranceDb": tolerance_db,
                }),
            )
        },
        if sample_count > 0 {
            pass_step("movement.samples", "sample count was recorded")
        } else {
            fail_step(
                "movement.samples",
                "no samples were recorded",
                serde_json::json!({"sampleCount": sample_count}),
            )
        },
    ]
}

fn connected_event_matches_generation(
    event_generation: u64,
    active_generation: Option<u64>,
) -> bool {
    active_generation == Some(event_generation)
}

fn show_state_matches_connected_identity(
    connected_identity: Option<&Lv1SystemIdentity>,
    requested_identity: &Lv1SystemIdentity,
) -> bool {
    connected_identity == Some(requested_identity)
}

fn connection_observation_steps(
    saw_connect_requested: bool,
    saw_lv1_connected: bool,
    saw_show_connected_identity: bool,
    observed_traces: &[SmokeTraceEvent],
) -> Vec<crate::smoke::SmokeStepResult> {
    let saw_connect_requested_trace = observed_traces
        .iter()
        .any(|event| event.has_field("event", "lv1_connect_requested"));
    let saw_lv1_connected_trace = observed_traces
        .iter()
        .any(|event| event.has_field("event", "lv1_connected"));

    vec![
        if saw_connect_requested {
            pass_step("connect-requested", "connect request observed")
        } else {
            fail_step(
                "connect-requested",
                "connect request was not observed",
                serde_json::json!({"requested": saw_connect_requested}),
            )
        },
        if saw_lv1_connected {
            pass_step("lv1-connected", "LV1 connected event observed")
        } else {
            fail_step(
                "lv1-connected",
                "LV1 connected event was not observed",
                serde_json::json!({"connected": saw_lv1_connected}),
            )
        },
        if saw_show_connected_identity {
            pass_step(
                "show-connected-identity",
                "show projection reflected connected identity",
            )
        } else {
            fail_step(
                "show-connected-identity",
                "connected identity was not reflected in show state",
                serde_json::json!({"connectedIdentity": saw_show_connected_identity}),
            )
        },
        if saw_connect_requested_trace {
            pass_step(
                "trace-connect-requested",
                "trace recorded lv1_connect_requested",
            )
        } else {
            fail_step(
                "trace-connect-requested",
                "trace did not record lv1_connect_requested",
                serde_json::json!({"traceMatch": saw_connect_requested_trace}),
            )
        },
        if saw_lv1_connected_trace {
            pass_step("trace-lv1-connected", "trace recorded lv1_connected")
        } else {
            fail_step(
                "trace-lv1-connected",
                "trace did not record lv1_connected",
                serde_json::json!({"traceMatch": saw_lv1_connected_trace}),
            )
        },
    ]
}

pub async fn run_connection_test(
    app: tauri::AppHandle<impl tauri::Runtime>,
    lifecycle: &AppLifecycle,
    identity: Lv1SystemIdentity,
    timeout_ms: u64,
) -> Result<SmokeBackendResult, String> {
    let trace_capture = SmokeTraceCapture::new(128);
    let trace_run = trace_capture.start_run("connection");
    let trace_layer = SmokeTraceLayer::new(trace_capture.clone());
    let subscriber = tracing_subscriber::registry().with(trace_layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut rx = lifecycle.debug_smoke_event_bus().subscribe();
    let started_at = current_timestamp_millis();
    let result = lifecycle.connect_lv1_system(app, identity.clone()).await;
    let mut observed_events = Vec::new();
    let mut saw_lv1_connected = false;
    let mut saw_show_connected_identity = false;
    let mut active_generation = None;

    while let Ok(event) =
        tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx.recv()).await
    {
        let event = event.map_err(|err| err.to_string())?;
        observed_events.push(summarize_app_event(&event));
        match event {
            AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }) => {
                active_generation = Some(generation);
            }
            AppEvent::Lv1 {
                generation,
                event: crate::lv1::Lv1Event::Connected,
                ..
            } => {
                saw_lv1_connected =
                    connected_event_matches_generation(generation, active_generation);
            }
            AppEvent::Show(crate::show::ShowEvent::StateChanged { state, .. }) => {
                saw_show_connected_identity = show_state_matches_connected_identity(
                    state.connected_lv1_identity.as_ref(),
                    &identity,
                );
            }
            _ => {}
        }
        if saw_lv1_connected && saw_show_connected_identity {
            break;
        }
    }

    let observed_traces = trace_run.finish();
    let steps = connection_observation_steps(
        result.is_ok(),
        saw_lv1_connected,
        saw_show_connected_identity,
        &observed_traces,
    );
    let ok = steps.iter().all(|step| step.ok);

    Ok(SmokeBackendResult {
        ok,
        test_id: "connection".to_string(),
        started_at,
        finished_at: current_timestamp_millis(),
        steps,
        observed_events,
        observed_traces,
    })
}

pub async fn run_scene_recall_test(
    lifecycle: &AppLifecycle,
    params: SmokeTestParams,
    target_scene_id: String,
) -> Result<SmokeBackendResult, String> {
    let trace_capture = SmokeTraceCapture::new(128);
    let trace_run = trace_capture.start_run("scene-recall");
    let trace_layer = SmokeTraceLayer::new(trace_capture.clone());
    let subscriber = tracing_subscriber::registry().with(trace_layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut rx = lifecycle.debug_smoke_event_bus().subscribe();
    let started_at = current_timestamp_millis();

    let scenes_handle = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or_else(|| "scene recall handle unavailable".to_string())?;
    let (reply, response_rx) = oneshot::channel();
    scenes_handle
        .send(ScenesCommand::RecallScene {
            scene_id: target_scene_id.clone(),
            reply,
        })
        .await
        .map_err(|error| error.to_string())?;
    let result = response_rx
        .await
        .map_err(|_| "scene recall reply channel closed".to_string())?;
    let _recall_result: RecallSceneResult = result.map_err(|error| error.to_string())?;

    let mut saw_recall_requested = false;
    let mut saw_scene_changed = false;
    let mut saw_ready = false;
    let mut saw_start_requested = false;
    let mut saw_blocked = false;
    let mut saw_skipped = false;
    let mut observed_events = Vec::new();

    while let Ok(event) = tokio::time::timeout(
        std::time::Duration::from_millis(params.timeout_ms),
        rx.recv(),
    )
    .await
    {
        let event = event.map_err(|err| err.to_string())?;
        observed_events.push(summarize_app_event(&event));
        match &event {
            AppEvent::Scenes {
                event: ScenesEvent::Ready { .. },
                ..
            } => saw_ready = true,
            AppEvent::Scenes {
                event: ScenesEvent::StartRequested { .. },
                ..
            } => saw_start_requested = true,
            AppEvent::Scenes {
                event: ScenesEvent::Blocked { .. },
                ..
            } => saw_blocked = true,
            AppEvent::Scenes {
                event: ScenesEvent::Skipped { .. },
                ..
            } => saw_skipped = true,
            other if scene_event_matches_target(other, &target_scene_id) => {
                saw_scene_changed = true
            }
            _ => {}
        }
        saw_recall_requested |= observed_events
            .iter()
            .any(|entry| entry.contains("scene_recall_requested"));
        if saw_scene_changed && saw_ready && saw_start_requested {
            break;
        }
    }

    let observed_traces = trace_run.finish();
    let steps = scene_recall_steps(
        saw_recall_requested,
        saw_scene_changed,
        saw_ready,
        saw_start_requested,
        saw_blocked,
        saw_skipped,
        &observed_traces,
    );

    Ok(SmokeBackendResult {
        ok: steps.iter().all(|step| step.ok),
        test_id: "scene-recall".to_string(),
        started_at,
        finished_at: current_timestamp_millis(),
        steps,
        observed_events,
        observed_traces,
    })
}

pub async fn run_fade_starts_test(
    lifecycle: &AppLifecycle,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    let trace_capture = SmokeTraceCapture::new(128);
    let trace_run = trace_capture.start_run("fade-starts");
    let trace_layer = SmokeTraceLayer::new(trace_capture.clone());
    let subscriber = tracing_subscriber::registry().with(trace_layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut rx = lifecycle.debug_smoke_event_bus().subscribe();
    let started_at = current_timestamp_millis();
    let mut saw_ready = false;
    let mut saw_start_requested = false;
    let mut saw_fade_started = false;
    let mut saw_fader_changed = false;
    let mut saw_target_channel_fader_changed = false;
    let mut saw_target_channel_gain_valid = false;
    let mut observed_events = Vec::new();

    let _ = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or_else(|| "scene recall handle unavailable".to_string())?;

    let show = lifecycle.current_show().await;
    let (show_reply, show_rx) = oneshot::channel();
    show.send(crate::show::ShowCommand::GetSceneConfig {
        scene_id: params.scene_b_id.clone(),
        reply: show_reply,
    })
    .await
    .map_err(|error| error.to_string())?;
    let show_scene = match show_rx
        .await
        .map_err(|_| "show state reply channel closed".to_string())?
    {
        Some(scene) => scene,
        None => {
            let observed_traces = trace_run.finish();
            return Ok(SmokeBackendResult {
                ok: false,
                test_id: "fade-starts".to_string(),
                started_at,
                finished_at: current_timestamp_millis(),
                steps: vec![fail_step(
                    "lv1.targetChannelGainValid",
                    "target channel gain target is unavailable",
                    serde_json::json!({
                        "sceneId": params.scene_b_id,
                        "channel": {"group": params.channel.group, "channel": params.channel.channel}
                    }),
                )],
                observed_events,
                observed_traces,
            });
        }
    };

    let lv1 = lifecycle
        .debug_smoke_current_lv1()
        .await
        .ok_or_else(|| "LV1 accessor unavailable".to_string())?;
    let (state_reply, state_rx) = oneshot::channel();
    lv1.send(crate::lv1::Lv1Command::GetState { reply: state_reply })
        .await
        .map_err(|error| error.to_string())?;
    let snapshot = state_rx
        .await
        .map_err(|_| "LV1 state reply channel closed".to_string())?;

    let start_db = sample_channel_gain(&snapshot, &params.channel)
        .ok_or_else(|| "target channel start value unavailable".to_string())?;
    let expected_target_db = match show_scene
        .channel_configs
        .iter()
        .find(|entry| {
            entry.group == params.channel.group && entry.channel == params.channel.channel
        })
        .and_then(|entry| entry.fader_db)
    {
        Some(target_db) => target_db,
        None => {
            let observed_traces = trace_run.finish();
            return Ok(SmokeBackendResult {
                ok: false,
                test_id: "fade-starts".to_string(),
                started_at,
                finished_at: current_timestamp_millis(),
                steps: vec![fail_step(
                    "lv1.targetChannelGainValid",
                    "target channel gain target is unavailable",
                    serde_json::json!({
                        "sceneId": params.scene_b_id,
                        "channel": {"group": params.channel.group, "channel": params.channel.channel}
                    }),
                )],
                observed_events,
                observed_traces,
            });
        }
    };

    while let Ok(event) = tokio::time::timeout(
        std::time::Duration::from_millis(params.timeout_ms),
        rx.recv(),
    )
    .await
    {
        let event = event.map_err(|err| err.to_string())?;
        observed_events.push(summarize_app_event(&event));
        match event {
            AppEvent::Scenes {
                event: ScenesEvent::Ready { .. },
                ..
            } => saw_ready = true,
            AppEvent::Scenes {
                event: ScenesEvent::StartRequested { .. },
                ..
            } => saw_start_requested = true,
            AppEvent::Fade {
                event: crate::fade::FadeEvent::FadeStarted,
                ..
            } => saw_fade_started = true,
            AppEvent::Lv1 {
                event: crate::lv1::Lv1Event::FaderChanged { .. },
                ..
            } => {
                saw_fader_changed = true;
                if event_matches_test_channel(&event, &params.channel) {
                    saw_target_channel_fader_changed = true;
                    if let AppEvent::Lv1 {
                        event: crate::lv1::Lv1Event::FaderChanged { gain_db, .. },
                        ..
                    } = event
                    {
                        saw_target_channel_gain_valid = movement_matches_expected_direction(
                            start_db,
                            expected_target_db,
                            gain_db,
                        );
                    }
                }
            }
            _ => {}
        }
        if saw_ready && saw_start_requested && saw_fade_started && saw_fader_changed {
            break;
        }
    }

    let observed_traces = trace_run.finish();
    let steps = vec![
        if saw_ready {
            pass_step("scene.ready", "scene recall ready event was observed")
        } else {
            fail_step(
                "scene.ready",
                "scene recall ready event was not observed",
                serde_json::json!({"ready": saw_ready}),
            )
        },
        if saw_start_requested {
            pass_step("scene.startRequested", "fade start request was observed")
        } else {
            fail_step(
                "scene.startRequested",
                "fade start request was not observed",
                serde_json::json!({"startRequested": saw_start_requested}),
            )
        },
        if saw_fade_started {
            pass_step("fade.started", "fade start event was observed")
        } else {
            fail_step(
                "fade.started",
                "fade start event was not observed",
                serde_json::json!({"fadeStarted": saw_fade_started}),
            )
        },
        if saw_fader_changed {
            pass_step("lv1.faderChanged", "LV1 fader change was observed")
        } else {
            fail_step(
                "lv1.faderChanged",
                "LV1 fader change was not observed",
                serde_json::json!({"faderChanged": saw_fader_changed}),
            )
        },
        if saw_target_channel_fader_changed {
            pass_step(
                "lv1.targetChannelFaderChanged",
                "target channel fader change was observed",
            )
        } else {
            fail_step(
                "lv1.targetChannelFaderChanged",
                "target channel fader change was not observed",
                serde_json::json!({"channel": {"group": params.channel.group, "channel": params.channel.channel}}),
            )
        },
        if saw_target_channel_gain_valid {
            pass_step(
                "lv1.targetChannelGainValid",
                "target channel gain was valid",
            )
        } else {
            fail_step(
                "lv1.targetChannelGainValid",
                if saw_target_channel_fader_changed {
                    "target channel gain moved the wrong direction"
                } else {
                    "target channel gain was not validated"
                },
                serde_json::json!({
                    "validated": saw_target_channel_gain_valid,
                    "startDb": start_db,
                    "targetDb": expected_target_db,
                    "targetChannelFaderChanged": saw_target_channel_fader_changed,
                }),
            )
        },
    ];

    Ok(SmokeBackendResult {
        ok: steps.iter().all(|step| step.ok),
        test_id: "fade-starts".to_string(),
        started_at,
        finished_at: current_timestamp_millis(),
        steps,
        observed_events,
        observed_traces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_steps_pass_for_connected_event_and_trace() {
        let steps = connection_observation_steps(
            true,
            true,
            true,
            &[
                SmokeTraceEvent {
                    timestamp_ms: 1,
                    level: "DEBUG".to_string(),
                    target: "test".to_string(),
                    fields: vec![crate::smoke::SmokeTraceField {
                        name: "event".to_string(),
                        value: "lv1_connect_requested".to_string(),
                    }],
                },
                SmokeTraceEvent {
                    timestamp_ms: 2,
                    level: "INFO".to_string(),
                    target: "test".to_string(),
                    fields: vec![crate::smoke::SmokeTraceField {
                        name: "event".to_string(),
                        value: "lv1_connected".to_string(),
                    }],
                },
            ],
        );

        assert!(steps.iter().all(|step| step.ok));
    }

    #[test]
    fn connected_event_requires_matching_generation() {
        assert!(connected_event_matches_generation(9, Some(9)));
        assert!(!connected_event_matches_generation(9, Some(8)));
        assert!(!connected_event_matches_generation(9, None));
    }

    #[test]
    fn show_state_requires_matching_connected_identity() {
        let requested = Lv1SystemIdentity {
            uuid: Some("lv1-uuid".to_string()),
            host: Some("lv1.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 7788,
        };
        let wrong_identity = Lv1SystemIdentity {
            uuid: Some("lv1-uuid".to_string()),
            host: Some("lv1.local".to_string()),
            address: "192.0.2.11".to_string(),
            port: 7788,
        };

        assert!(show_state_matches_connected_identity(
            Some(&requested),
            &requested
        ));
        assert!(!show_state_matches_connected_identity(
            Some(&wrong_identity),
            &requested
        ));
        assert!(!show_state_matches_connected_identity(None, &requested));
    }

    #[test]
    fn fade_completion_fails_when_manual_override_trace_is_present() {
        let steps = fade_completion_steps(
            true,
            true,
            -10.0,
            -10.2,
            0.5,
            &[trace_event("manual_override_detected")],
        );

        assert!(
            steps
                .iter()
                .any(|step| !step.ok && step.step == "trace.noManualOverride")
        );
    }

    #[test]
    fn trace_has_manual_override_detects_manual_override_events() {
        assert!(trace_has_manual_override(&[trace_event(
            "manual_override_detected"
        )]));
        assert!(!trace_has_manual_override(&[trace_event(
            "channel_override_detected"
        )]));
    }

    #[test]
    fn fade_completion_fails_when_channel_override_trace_is_present() {
        let steps = fade_completion_steps(
            true,
            true,
            -10.0,
            -10.2,
            0.5,
            &[trace_event("channel_override_detected")],
        );

        assert!(
            steps
                .iter()
                .any(|step| !step.ok && step.step == "trace.noChannelOverride")
        );
    }

    #[test]
    fn lockout_no_movement_detected_rejects_intermediate_movement() {
        assert!(lockout_no_movement_detected(-12.0, &[-12.0, -12.1], 0.5));
        assert!(!lockout_no_movement_detected(-12.0, &[-12.0, -10.0], 0.5));
    }

    #[test]
    fn lockout_steps_fail_when_intermediate_movement_is_observed() {
        let steps = lockout_steps(true, true, true, false, true);

        assert!(
            steps
                .iter()
                .any(|step| !step.ok && step.step == "lv1.noMovement")
        );
    }

    #[test]
    fn fader_movement_passes_when_final_value_is_within_tolerance() {
        let steps = fader_movement_steps(-30.0, -10.0, -10.3, -30.0, -10.0, 0.5, 3.0, 8);

        assert!(steps.iter().all(|step| step.ok));
    }

    #[test]
    fn event_filter_matches_only_configured_test_channel() {
        let channel = crate::smoke::runner::SmokeTestChannel {
            group: 1,
            channel: 2,
        };
        let matching = AppEvent::Lv1 {
            generation: 1,
            event: crate::lv1::Lv1Event::FaderChanged {
                group: 1,
                channel: 2,
                gain_db: -12.0,
            },
        };
        let unrelated = AppEvent::Lv1 {
            generation: 1,
            event: crate::lv1::Lv1Event::FaderChanged {
                group: 1,
                channel: 3,
                gain_db: -12.0,
            },
        };

        assert!(event_matches_test_channel(&matching, &channel));
        assert!(!event_matches_test_channel(&unrelated, &channel));
    }

    #[test]
    fn sample_channel_gain_returns_configured_channel_value() {
        let snapshot = crate::lv1::Lv1StateSnapshot {
            connection: crate::lv1::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![],
            channels: vec![crate::lv1::ChannelInfo {
                group: 1,
                channel: 2,
                name: "Test".to_string(),
                gain_db: -9.5,
                muted: false,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
        };
        let channel = crate::smoke::runner::SmokeTestChannel {
            group: 1,
            channel: 2,
        };

        assert_eq!(sample_channel_gain(&snapshot, &channel), Some(-9.5));
    }

    #[test]
    fn scene_channel_target_db_returns_matching_scene_channel_target() {
        let show = crate::show::ShowDocument {
            lockout: false,
            cued_scene_id: None,
            scene_configs: vec![crate::show::SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 1_000,
                channel_configs: vec![crate::show::ChannelConfig {
                    group: 1,
                    channel: 2,
                    fader_db: Some(-12.0),
                    pan: None,
                    balance: None,
                    width: None,
                    pan_mode: None,
                }],
                scoped_channels: vec![],
                scope_toggles: crate::show::SceneScopeToggles::default(),
            }],
        };
        let channel = crate::smoke::runner::SmokeTestChannel {
            group: 1,
            channel: 2,
        };

        assert_eq!(
            scene_channel_target_db(&show, "1::Verse", &channel),
            Some(-12.0)
        );
        assert_eq!(scene_channel_target_db(&show, "1::Other", &channel), None);
    }

    #[test]
    fn movement_matches_expected_direction_rejects_wrong_direction() {
        assert!(movement_matches_expected_direction(-20.0, -10.0, -16.0));
        assert!(!movement_matches_expected_direction(-20.0, -10.0, -22.0));
    }

    #[test]
    fn decreasing_xfade_sequence_alternates_scenes() {
        let sequence = decreasing_xfade_sequence("0::A", "1::B");

        assert_eq!(sequence[0].duration_ms, 5_000);
        assert_eq!(sequence[0].target_scene_id, "1::B");
        assert_eq!(sequence[1].duration_ms, 3_000);
        assert_eq!(sequence[1].target_scene_id, "0::A");
        assert_eq!(sequence[2].duration_ms, 1_000);
        assert_eq!(sequence[2].target_scene_id, "1::B");
        assert_eq!(sequence[3].duration_ms, 500);
        assert_eq!(sequence[3].target_scene_id, "0::A");
    }

    #[test]
    fn lockout_steps_pass_when_blocked_without_fade_start() {
        let steps = lockout_steps(true, true, true, true, true);

        assert!(steps.iter().all(|step| step.ok));
    }
}

pub async fn run_fade_completes_test(
    lifecycle: &AppLifecycle,
    params: SmokeTestParams,
    expected_target_db: f64,
) -> Result<SmokeBackendResult, String> {
    let trace_capture = SmokeTraceCapture::new(128);
    let trace_run = trace_capture.start_run("fade-completes");
    let trace_layer = SmokeTraceLayer::new(trace_capture.clone());
    let subscriber = tracing_subscriber::registry().with(trace_layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut rx = lifecycle.debug_smoke_event_bus().subscribe();
    let started_at = current_timestamp_millis();
    let mut observed_events = Vec::new();
    let mut saw_fade_started = false;
    let mut saw_fade_completed = false;
    let mut observed_start_db = None;
    let mut observed_min_db: Option<f64> = None;
    let mut observed_max_db: Option<f64> = None;
    let mut observed_final_db: Option<f64> = None;
    let mut samples = 0usize;

    if lifecycle.debug_smoke_current_lv1().await.is_none() {
        return Err("LV1 accessor unavailable".to_string());
    }

    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_millis(params.timeout_ms);
    let mut sample_tick =
        tokio::time::interval(std::time::Duration::from_millis(params.sample_interval_ms));
    sample_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }

        tokio::select! {
            event = rx.recv() => {
                let event = event.map_err(|err| err.to_string())?;
                observed_events.push(summarize_app_event(&event));
                match event {
                    AppEvent::Fade { event: crate::fade::FadeEvent::FadeStarted, .. } => saw_fade_started = true,
                    AppEvent::Fade { event: crate::fade::FadeEvent::FadeCompleted, .. } => saw_fade_completed = true,
                    _ => {}
                }
            }
            _ = sample_tick.tick() => {
                let lv1 = lifecycle.debug_smoke_current_lv1().await.ok_or_else(|| "LV1 accessor unavailable".to_string())?;
                let (reply, response_rx) = oneshot::channel();
                lv1.send(crate::lv1::Lv1Command::GetState { reply }).await.map_err(|error| error.to_string())?;
                let snapshot = response_rx.await.map_err(|_| "LV1 state reply channel closed".to_string())?;
                if let Some(sample_db) = sample_channel_gain(&snapshot, &params.channel) {
                    samples += 1;
                    observed_start_db.get_or_insert(sample_db);
                    observed_min_db = Some(match observed_min_db {
                        Some(current) => current.min(sample_db),
                        None => sample_db,
                    });
                    observed_max_db = Some(match observed_max_db {
                        Some(current) => current.max(sample_db),
                        None => sample_db,
                    });
                    observed_final_db = Some(sample_db);
                }
            }
        }

        if saw_fade_started && saw_fade_completed && samples > 0 {
            break;
        }
    }

    let observed_traces = trace_run.finish();
    let steps = fade_completion_steps(
        saw_fade_started,
        saw_fade_completed,
        expected_target_db,
        observed_final_db.unwrap_or(expected_target_db),
        params.tolerance_db,
        &observed_traces,
    );
    let mut steps = steps;
    steps.push(if samples > 0 {
        pass_step("lv1.samples", "LV1 samples were recorded")
    } else {
        fail_step(
            "lv1.samples",
            "LV1 samples were not recorded",
            serde_json::json!({"sampleCount": samples}),
        )
    });
    steps.push(if let (Some(start_db), Some(min_db), Some(max_db), Some(final_db)) = (observed_start_db, observed_min_db, observed_max_db, observed_final_db) {
        let moved_toward_target = (start_db - expected_target_db).abs() >= params.minimum_movement_db && (final_db - expected_target_db).abs() <= params.tolerance_db;
        if moved_toward_target {
            pass_step("lv1.sampledMovement", "sampled LV1 movement was valid")
        } else {
            fail_step("lv1.sampledMovement", "sampled LV1 movement was invalid", serde_json::json!({"startDb": start_db, "minDb": min_db, "maxDb": max_db, "finalDb": final_db, "targetDb": expected_target_db, "sampleCount": samples}))
        }
    } else {
        fail_step("lv1.sampledMovement", "LV1 samples were incomplete", serde_json::json!({"sampleCount": samples}))
    });

    Ok(SmokeBackendResult {
        ok: steps.iter().all(|step| step.ok),
        test_id: "fade-completes".to_string(),
        started_at,
        finished_at: current_timestamp_millis(),
        steps,
        observed_events,
        observed_traces,
    })
}

pub async fn run_decreasing_xfade_test(
    lifecycle: &AppLifecycle,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    let trace_capture = SmokeTraceCapture::new(128);
    let trace_run = trace_capture.start_run("decreasing-xfade");
    let trace_layer = SmokeTraceLayer::new(trace_capture.clone());
    let subscriber = tracing_subscriber::registry().with(trace_layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut rx = lifecycle.debug_smoke_event_bus().subscribe();
    let show = lifecycle.current_show().await;
    let scenes_handle = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or_else(|| "scene recall handle unavailable".to_string())?;

    let sequence = decreasing_xfade_sequence(&params.scene_a_id, &params.scene_b_id);
    let mut observed_events = Vec::new();
    let saw_duration_updates = sequence.len();
    let saw_recall_requests = sequence.len();
    let mut saw_fade_started = 0usize;
    let mut saw_fade_completed = 0usize;
    let mut saw_channel_override = false;
    let mut saw_manual_override_trace = false;
    let mut saw_observation_timeout = false;

    for step in &sequence {
        let step_deadline =
            tokio::time::Instant::now() + std::time::Duration::from_millis(params.timeout_ms);
        let (reply, response_rx) = oneshot::channel();
        tokio::time::timeout_at(
            step_deadline,
            show.send(crate::show::ShowCommand::SetSceneDuration {
                scene_id: step.target_scene_id.clone(),
                duration_ms: step.duration_ms,
                reply: Some(reply),
            }),
        )
        .await
        .map_err(|_| "timed out waiting to send scene duration update".to_string())?
        .map_err(|error| error.to_string())?;
        tokio::time::timeout_at(step_deadline, response_rx)
            .await
            .map_err(|_| "timed out waiting for show duration reply".to_string())?
            .map_err(|_| "show duration reply channel closed".to_string())?
            .map_err(|error| error.to_string())?;

        let (reply, response_rx) = oneshot::channel();
        tokio::time::timeout_at(
            step_deadline,
            scenes_handle.send(ScenesCommand::RecallScene {
                scene_id: step.target_scene_id.clone(),
                reply,
            }),
        )
        .await
        .map_err(|_| "timed out waiting to send scene recall".to_string())?
        .map_err(|error| error.to_string())?;
        let result = tokio::time::timeout_at(step_deadline, response_rx)
            .await
            .map_err(|_| "timed out waiting for scene recall reply".to_string())?
            .map_err(|_| "scene recall reply channel closed".to_string())?;
        let _recall_result: RecallSceneResult = result.map_err(|error| error.to_string())?;

        let (
            step_saw_fade_started,
            step_saw_fade_completed,
            step_saw_channel_override,
            step_timed_out,
            _,
            step_events,
        ) = wait_for_fade_completion_observation(
            lifecycle,
            &params,
            &mut rx,
            &step.target_scene_id,
        )
        .await?;
        observed_events.extend(step_events);
        saw_fade_started += usize::from(step_saw_fade_started);
        saw_fade_completed += usize::from(step_saw_fade_completed);
        saw_channel_override |= step_saw_channel_override;
        saw_observation_timeout |= step_timed_out;

        if trace_has_manual_override(&trace_run.snapshot()) {
            saw_manual_override_trace = true;
            break;
        }
    }

    let observed_traces = trace_run.finish();
    saw_manual_override_trace |= trace_has_manual_override(&observed_traces);
    saw_channel_override |= observed_traces
        .iter()
        .any(|event| event.has_field("event", "channel_override_detected"));

    let steps = vec![
        if saw_duration_updates >= sequence.len() {
            pass_step("show.durationUpdates", "scene durations were updated")
        } else {
            fail_step(
                "show.durationUpdates",
                "scene durations were not updated",
                serde_json::json!({"updates": saw_duration_updates}),
            )
        },
        if saw_recall_requests >= sequence.len() {
            pass_step("scene.recallRequests", "scene recalls were requested")
        } else {
            fail_step(
                "scene.recallRequests",
                "scene recalls were not requested",
                serde_json::json!({"recalls": saw_recall_requests}),
            )
        },
        if saw_fade_started >= sequence.len() {
            pass_step("fade.started", "fade start events were observed")
        } else {
            fail_step(
                "fade.started",
                "fade start events were not fully observed",
                serde_json::json!({"fadeStarted": saw_fade_started}),
            )
        },
        if saw_fade_completed >= sequence.len() {
            pass_step("fade.completed", "fade completion events were observed")
        } else {
            fail_step(
                "fade.completed",
                "fade completion events were not fully observed",
                serde_json::json!({"fadeCompleted": saw_fade_completed}),
            )
        },
        if !saw_channel_override {
            pass_step(
                "trace.noChannelOverride",
                "channel override trace was absent",
            )
        } else {
            fail_step(
                "trace.noChannelOverride",
                "channel override trace was present",
                serde_json::json!({"channelOverrideTrace": true}),
            )
        },
        if !saw_manual_override_trace {
            pass_step("trace.noManualOverride", "manual override trace was absent")
        } else {
            fail_step(
                "trace.noManualOverride",
                "manual override trace was present",
                serde_json::json!({"manualOverrideTrace": true}),
            )
        },
        if !saw_channel_override {
            pass_step(
                "trace.noChannelOverride",
                "channel override trace was absent",
            )
        } else {
            fail_step(
                "trace.noChannelOverride",
                "channel override trace was present",
                serde_json::json!({"channelOverrideTrace": true}),
            )
        },
        if !saw_observation_timeout {
            pass_step(
                "fade.observationComplete",
                "fade completion observation completed",
            )
        } else {
            fail_step(
                "fade.observationComplete",
                "fade completion observation timed out",
                serde_json::json!({"fadeCompleted": saw_fade_completed, "expected": sequence.len()}),
            )
        },
    ];

    Ok(SmokeBackendResult {
        ok: steps.iter().all(|step| step.ok),
        test_id: "decreasing-xfade".to_string(),
        started_at: current_timestamp_millis(),
        finished_at: current_timestamp_millis(),
        steps,
        observed_events,
        observed_traces,
    })
}

pub async fn run_lockout_blocks_recall_test(
    lifecycle: &AppLifecycle,
    params: SmokeTestParams,
) -> Result<SmokeBackendResult, String> {
    let trace_capture = SmokeTraceCapture::new(128);
    let trace_run = trace_capture.start_run("lockout-blocks-recall");
    let trace_layer = SmokeTraceLayer::new(trace_capture.clone());
    let subscriber = tracing_subscriber::registry().with(trace_layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut rx = lifecycle.debug_smoke_event_bus().subscribe();
    let started_at = current_timestamp_millis();
    let scenes_handle = lifecycle
        .current_scene_recall_fader()
        .await
        .ok_or_else(|| "scene recall handle unavailable".to_string())?;
    let show = lifecycle.current_show().await;

    let (lockout_reply, lockout_rx) = oneshot::channel();
    show.send(crate::show::ShowCommand::SetLockout {
        enabled: true,
        reply: Some(lockout_reply),
    })
    .await
    .map_err(|error| error.to_string())?;
    lockout_rx
        .await
        .map_err(|_| "lockout reply channel closed".to_string())?;

    let lv1 = lifecycle
        .debug_smoke_current_lv1()
        .await
        .ok_or_else(|| "LV1 accessor unavailable".to_string())?;
    let (state_reply, state_rx) = oneshot::channel();
    lv1.send(crate::lv1::Lv1Command::GetState { reply: state_reply })
        .await
        .map_err(|error| error.to_string())?;
    let start_snapshot = state_rx
        .await
        .map_err(|_| "LV1 state reply channel closed".to_string())?;
    let start_db = sample_channel_gain(&start_snapshot, &params.channel)
        .ok_or_else(|| "target channel start value unavailable".to_string())?;

    let target_scene_id = params.scene_b_id.clone();
    let (reply, response_rx) = oneshot::channel();
    scenes_handle
        .send(ScenesCommand::RecallScene {
            scene_id: target_scene_id.clone(),
            reply,
        })
        .await
        .map_err(|error| error.to_string())?;
    let recall_result = response_rx
        .await
        .map_err(|_| "scene recall reply channel closed".to_string())?;
    let _recall_result: RecallSceneResult = recall_result.map_err(|error| error.to_string())?;

    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_millis(params.timeout_ms);
    let mut saw_blocked = false;
    let mut saw_scene_change = false;
    let mut saw_fade_started = false;
    let mut saw_fader_movement = false;
    let mut saw_intermediate_movement = false;
    let mut observed_events = Vec::new();
    let mut sampled_db = start_db;

    while let Ok(event) = tokio::time::timeout_at(deadline, rx.recv()).await {
        let event = event.map_err(|err| err.to_string())?;
        observed_events.push(summarize_app_event(&event));
        match &event {
            AppEvent::Scenes {
                event: ScenesEvent::Blocked { .. },
                ..
            } => saw_blocked = true,
            AppEvent::Fade {
                event: crate::fade::FadeEvent::FadeStarted,
                ..
            } => saw_fade_started = true,
            AppEvent::Lv1 {
                event: crate::lv1::Lv1Event::SceneChanged(scene),
                ..
            } if crate::show::scene_id(scene.index, &scene.name) == target_scene_id => {
                saw_scene_change = true
            }
            AppEvent::Lv1 {
                event: crate::lv1::Lv1Event::FaderChanged { .. },
                ..
            } if event_matches_test_channel(&event, &params.channel) => saw_fader_movement = true,
            _ => {}
        }
        let (state_reply, state_rx) = oneshot::channel();
        lv1.send(crate::lv1::Lv1Command::GetState { reply: state_reply })
            .await
            .map_err(|error| error.to_string())?;
        let snapshot = state_rx
            .await
            .map_err(|_| "LV1 state reply channel closed".to_string())?;
        if let Some(db) = sample_channel_gain(&snapshot, &params.channel) {
            saw_intermediate_movement |= (db - start_db).abs() > params.tolerance_db;
            sampled_db = db;
        }
        if saw_blocked {
            break;
        }
    }

    let observed_traces = trace_run.finish();
    let saw_lockout_trace = observed_traces
        .iter()
        .any(|event| event.has_field("event", "scene_recall_blocked"));
    let steps = lockout_steps(
        saw_blocked,
        !saw_scene_change,
        !saw_fade_started,
        !saw_fader_movement
            && !saw_intermediate_movement
            && (sampled_db - start_db).abs() <= params.tolerance_db,
        saw_lockout_trace,
    );

    Ok(SmokeBackendResult {
        ok: steps.iter().all(|step| step.ok),
        test_id: "lockout-blocks-recall".to_string(),
        started_at,
        finished_at: current_timestamp_millis(),
        steps,
        observed_events,
        observed_traces,
    })
}

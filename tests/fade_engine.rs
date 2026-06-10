use advanced_show_control::fade::actor::spawn_engine;
use advanced_show_control::fade::curve::FadeCurve;
use advanced_show_control::fade::events::FadeEvent;
use advanced_show_control::fade::handle::FadeEngineHandle;
use advanced_show_control::fade::types::{
    FadeConfig, FadeParameter, FadeSceneIdentity, FadeTarget, FadeTargetKey,
};
use advanced_show_control::lv1::actor::spawn_actor;
use advanced_show_control::lv1::tcp::{FrameDecoder, decode_frame_payload, encode_frame};
use advanced_show_control::osc::OscArg;
use advanced_show_control::runtime::commands::AppCommandBus;
use advanced_show_control::runtime::events::{AppEvent, AppEventBus};
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;

fn lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
    encode_frame(address, args).unwrap()
}

fn scene(index: i32, name: &str) -> FadeSceneIdentity {
    FadeSceneIdentity {
        index,
        name: name.to_string(),
    }
}

fn fade_config(scene: FadeSceneIdentity, targets: Vec<FadeTarget>, duration_ms: u64) -> FadeConfig {
    FadeConfig {
        scene,
        targets,
        duration_ms,
        curve: FadeCurve::Linear,
    }
}

#[test]
fn fade_target_key_includes_parameter() {
    let gain_target = FadeTarget {
        group: 0,
        channel: 1,
        parameter: FadeParameter::FaderDb,
        target: -12.0,
    };
    let pan_target = FadeTarget {
        group: 0,
        channel: 1,
        parameter: FadeParameter::Pan,
        target: 0.0,
    };

    assert_eq!(
        gain_target.key(),
        FadeTargetKey {
            group: 0,
            channel: 1,
            parameter: FadeParameter::FaderDb,
        }
    );
    assert_ne!(gain_target.key(), pan_target.key());
}

fn channels_args() -> Vec<OscArg> {
    let mut args = vec![OscArg::Int(1)];
    args.push(OscArg::String("Ch 1".to_string()));
    args.push(OscArg::Int(0));
    args.push(OscArg::Int(0));
    args.push(OscArg::Double(-8.0));
    for _ in 0..15 {
        args.push(OscArg::Int(0));
    }
    args
}

fn two_channels_args() -> Vec<OscArg> {
    let mut args = vec![OscArg::Int(2)];
    for (name, channel) in [("Ch 1", 0), ("Ch 2", 1)] {
        args.push(OscArg::String(name.to_string()));
        args.push(OscArg::Int(0));
        args.push(OscArg::Int(channel));
        args.push(OscArg::Double(-8.0));
        for _ in 0..15 {
            args.push(OscArg::Int(0));
        }
    }
    args
}

async fn no_global_fade_completed_for(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    timeout: std::time::Duration,
) {
    let completed = tokio::time::timeout(timeout, async {
        loop {
            match events.recv().await {
                Ok(AppEvent::Fade(FadeEvent::FadeCompleted)) => return true,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(!completed, "Intro should still be active");
}

async fn wait_for_app_fade_event(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    timeout: std::time::Duration,
    pred: impl Fn(&FadeEvent) -> bool,
) -> FadeEvent {
    tokio::time::timeout(timeout, async {
        loop {
            match events.recv().await {
                Ok(AppEvent::Fade(event)) if pred(&event) => return event,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("event stream ended without matching event")
                }
            }
        }
    })
    .await
    .expect("timed out waiting for fade event")
}

async fn spawn_runtime_for_test(
    lv1: advanced_show_control::lv1::handle::Lv1ActorHandle,
    event_bus: AppEventBus,
) -> (AppCommandBus, FadeEngineHandle) {
    let bus = AppCommandBus::new(event_bus.clone());
    bus.set_lv1(Some(lv1)).await;
    let engine = spawn_engine(bus.clone(), event_bus);
    bus.set_fade(Some(engine.clone())).await;
    (bus, engine)
}

#[tokio::test]
async fn zero_duration_fade_sends_final_gain_without_running_state() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (gain_tx, gain_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();

        let mut buf = [0_u8; 1024];
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
                        let msg = decode_frame_payload(&frame).unwrap();
                        if msg.address == "/Set/Track/Out/Gain"
                            && let (
                                Some(OscArg::Int(group)),
                                Some(OscArg::Int(channel)),
                                Some(OscArg::Double(gain_db)),
                            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
                        {
                            let _ = gain_tx.send((*group, *channel, *gain_db));
                        }
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => panic!("server read failed: {err}"),
            }
        }
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.5,
            }],
            0,
        ))
        .await
        .unwrap();

    let first_gain = tokio::task::spawn_blocking(move || {
        gain_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("zero-duration fade did not send final gain")
    })
    .await
    .unwrap();
    assert_eq!(first_gain, (0, 0, -12.5));

    let first_event = wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |_| true,
    )
    .await;
    assert!(
        matches!(
            first_event,
            FadeEvent::ChannelCompleted {
                group: 0,
                channel: 0
            }
        ),
        "zero-duration fade should complete channels without entering running state first, got {first_event:?}"
    );
}

#[tokio::test]
async fn non_fader_targets_do_not_send_gain_commands_before_parameter_support() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (gain_tx, gain_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();

        let mut buf = [0_u8; 1024];
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
                        let msg = decode_frame_payload(&frame).unwrap();
                        if msg.address == "/Set/Track/Out/Gain" {
                            let _ = gain_tx.send(msg.address);
                        }
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => panic!("server read failed: {err}"),
            }
        }
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
                target: -12.0,
            }],
            100,
        ))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let result = tokio::task::spawn_blocking(move || {
        gain_rx.recv_timeout(std::time::Duration::from_millis(100))
    })
    .await
    .unwrap();

    assert!(
        result.is_err(),
        "pan-only fade should not send gain commands"
    );
    no_global_fade_completed_for(&mut app_events, std::time::Duration::from_millis(150)).await;

    let started = tokio::time::timeout(std::time::Duration::from_millis(150), async {
        loop {
            match app_events.recv().await {
                Ok(AppEvent::Fade(FadeEvent::FadeStarted)) => return true,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    assert!(!started, "pan-only fade should not emit FadeStarted");
}

#[tokio::test]
async fn zero_duration_non_fader_targets_do_not_emit_fade_completed() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_lv1(Some(lv1)).await;
    let engine = spawn_engine(command_bus.clone(), event_bus.clone());
    command_bus.set_fade(Some(engine.clone())).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
                target: -12.0,
            }],
            0,
        ))
        .await
        .unwrap();

    let completed = tokio::time::timeout(std::time::Duration::from_millis(150), async {
        loop {
            match app_events.recv().await {
                Ok(AppEvent::Fade(FadeEvent::FadeCompleted)) => return true,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        !completed,
        "zero-duration pan-only fade should not emit FadeCompleted"
    );
}

#[tokio::test]
async fn same_scene_non_fader_targets_do_not_finish_active_fader_fade() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(600));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let intro = scene(1, "Intro");
    engine
        .start_fade(fade_config(
            intro.clone(),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.0,
            }],
            1_000,
        ))
        .await
        .unwrap();

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |event| matches!(event, FadeEvent::FadeStarted),
    )
    .await;

    engine
        .start_fade(fade_config(
            intro,
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
                target: -12.0,
            }],
            0,
        ))
        .await
        .unwrap();

    no_global_fade_completed_for(&mut app_events, std::time::Duration::from_millis(250)).await;
}

#[tokio::test]
async fn engine_emits_fade_started_and_completed() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(3));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            duration_ms: 500,
            curve: FadeCurve::Linear,
        })
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(e, FadeEvent::FadeCompleted)
    })
    .await;
}

#[tokio::test]
async fn engine_abort_all_stops_fade() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(5));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            duration_ms: 10_000,
            curve: FadeCurve::Linear,
        })
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = engine.abort_all().await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(2), |e| {
        matches!(e, FadeEvent::FadeAborted)
    })
    .await;
}

#[tokio::test]
async fn engine_detects_manual_override() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(400));
        stream
            .write_all(&lv1_frame(
                "/Notify/Track/Out/Gain",
                &[
                    OscArg::Int(0),
                    OscArg::Int(0),
                    OscArg::Double(0.0),
                    OscArg::True,
                ],
            ))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(3));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -20.0,
            }],
            duration_ms: 10_000,
            curve: FadeCurve::Linear,
        })
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(e, FadeEvent::ChannelOverride { .. })
    })
    .await;
}

#[tokio::test]
async fn different_scene_fade_while_running_replaces_previous_channel() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(5));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            duration_ms: 30_000,
            curve: FadeCurve::Linear,
        })
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = engine
        .start_fade(FadeConfig {
            scene: FadeSceneIdentity {
                index: 2,
                name: "Verse".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            duration_ms: 500,
            curve: FadeCurve::Linear,
        })
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(e, FadeEvent::FadeCompleted)
    })
    .await;
}

#[tokio::test]
async fn different_scene_fade_does_not_cancel_unrelated_channel() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &two_channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(5));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            30_000,
        ))
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = engine
        .start_fade(fade_config(
            scene(2, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            500,
        ))
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(2), |e| {
        matches!(
            e,
            FadeEvent::ChannelCompleted {
                group: 0,
                channel: 1
            }
        )
    })
    .await;

    no_global_fade_completed_for(&mut app_events, std::time::Duration::from_millis(150)).await;
}

#[tokio::test]
async fn recalling_same_scene_finishes_only_that_scene_channels() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &two_channels_args()))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(5));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            30_000,
        ))
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = engine
        .start_fade(fade_config(
            scene(2, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            30_000,
        ))
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            30_000,
        ))
        .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(2), |e| {
        matches!(
            e,
            FadeEvent::ChannelCompleted {
                group: 0,
                channel: 0
            }
        )
    })
    .await;

    no_global_fade_completed_for(&mut app_events, std::time::Duration::from_millis(500)).await;
}

#[tokio::test]
async fn replacement_fade_starts_from_active_mid_fade_value() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (gain_tx, gain_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        use std::io::Read;

        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &channels_args()))
            .unwrap();

        let mut buf = [0_u8; 1024];
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
                        let msg = decode_frame_payload(&frame).unwrap();
                        if msg.address == "/Set/Track/Out/Gain"
                            && let (
                                Some(OscArg::Int(group)),
                                Some(OscArg::Int(channel)),
                                Some(OscArg::Double(gain_db)),
                            ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
                        {
                            let _ = gain_tx.send((*group, *channel, *gain_db));
                        }
                    }
                }
                Err(err)
                    if err.kind() == std::io::ErrorKind::WouldBlock
                        || err.kind() == std::io::ErrorKind::TimedOut => {}
                Err(err) => panic!("server read failed: {err}"),
            }
        }
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            1_000,
        ))
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    while gain_rx.try_recv().is_ok() {}

    let _ = engine
        .start_fade(fade_config(
            scene(2, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            1_000,
        ))
        .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let replacement_first_gain = tokio::task::spawn_blocking(move || {
        loop {
            let (group, channel, gain_db) = gain_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("replacement fade did not send gain");
            if group == 0 && channel == 0 {
                return gain_db;
            }
        }
    })
    .await
    .unwrap();

    assert!(
        replacement_first_gain < -12.0,
        "replacement fade should continue from the active mid-fade value, got {replacement_first_gain}"
    );
}

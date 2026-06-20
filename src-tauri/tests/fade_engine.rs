use advanced_show_control::fade::{
    FadeCommand, FadeConfig, FadeCurve, FadeEngineHandle, FadeEvent, FadeParameter,
    FadeSceneIdentity, FadeTarget, FadeTargetKey, build_engine,
};
use advanced_show_control::lv1::{
    Lv1ActorHandle, Lv1Event, Lv1Frame, build_actor, decode_frame_payload, encode_frame,
};
use advanced_show_control::osc::OscArg;
use advanced_show_control::runtime::events::{AppEvent, AppEventBus};
use advanced_show_control::runtime::generation::RuntimeGeneration;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;

fn lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
    encode_frame(address, args).unwrap()
}

fn build_and_spawn_actor(
    host: String,
    port: u16,
    event_bus: AppEventBus,
    generation: u64,
) -> Lv1ActorHandle {
    let (handle, task) = build_actor(host, port, event_bus, generation);
    task.spawn();
    handle
}

#[derive(Default)]
struct TestFrameDecoder {
    buffer: Vec<u8>,
}

impl TestFrameDecoder {
    fn push(&mut self, bytes: &[u8]) -> Vec<Lv1Frame> {
        const HEADER_LEN: usize = 8;

        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        while self.buffer.len() >= 4 + HEADER_LEN {
            let payload_len = u32::from_be_bytes(self.buffer[0..4].try_into().unwrap()) as usize;
            let total_len = 4 + HEADER_LEN + payload_len;
            if self.buffer.len() < total_len {
                break;
            }

            let mut header = [0_u8; HEADER_LEN];
            header.copy_from_slice(&self.buffer[4..4 + HEADER_LEN]);
            let payload = self.buffer[4 + HEADER_LEN..total_len].to_vec();
            self.buffer.drain(..total_len);
            frames.push(Lv1Frame { header, payload });
        }
        frames
    }
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
                Ok(AppEvent::Fade {
                    event: FadeEvent::FadeCompleted,
                    ..
                }) => return true,
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
                Ok(AppEvent::Fade { event, .. }) if pred(&event) => return event,
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

async fn wait_for_app_event(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    timeout: std::time::Duration,
    pred: impl Fn(&AppEvent) -> bool,
) {
    tokio::time::timeout(timeout, async {
        loop {
            match events.recv().await {
                Ok(event) if pred(&event) => return,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("event stream ended without matching app event")
                }
            }
        }
    })
    .await
    .expect("timed out waiting for app event");
}

async fn spawn_runtime_for_test(
    lv1: Lv1ActorHandle,
    event_bus: AppEventBus,
) -> (RuntimeGeneration, FadeEngineHandle) {
    let runtime_generation = RuntimeGeneration::new();
    let (engine, task, peers) = build_engine(runtime_generation.clone(), event_bus, 0);
    peers.set_lv1(lv1);
    task.spawn();
    (runtime_generation, engine)
}

async fn start_fade(engine: &FadeEngineHandle, config: FadeConfig) -> Result<(), String> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    engine
        .send(FadeCommand::RecallSceneFade {
            config,
            expected_generation: None,
            reply: Some(reply),
        })
        .await
        .map_err(|_| "fade command channel closed".to_string())?;
    rx.await
        .map_err(|_| "fade reply channel closed".to_string())?
        .map_err(|err| err.to_string())
}

async fn abort_all(engine: &FadeEngineHandle) -> Result<(), String> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    engine
        .send(FadeCommand::AbortAll { reply: Some(reply) })
        .await
        .map_err(|_| "fade command channel closed".to_string())?;
    rx.await
        .map_err(|_| "fade reply channel closed".to_string())?
        .map_err(|err| err.to_string())
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
        let mut decoder = TestFrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]) {
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_runtime_generation, engine) =
        spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.5,
            }],
            0,
        ),
    )
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
                channel: 0,
                parameter: FadeParameter::FaderDb,
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
        let mut decoder = TestFrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]) {
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_runtime_generation, engine) =
        spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
                target: -12.0,
            }],
            100,
        ),
    )
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
    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(150),
        |event| matches!(event, FadeEvent::FadeStarted),
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_secs(2),
        |event| matches!(event, FadeEvent::FadeCompleted),
    )
    .await;
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let runtime_generation = RuntimeGeneration::new();
    let (engine, task, peers) = build_engine(runtime_generation, event_bus.clone(), 0);
    peers.set_lv1(lv1);
    task.spawn();
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
                target: -12.0,
            }],
            0,
        ),
    )
    .await
    .unwrap();

    let completed = tokio::time::timeout(std::time::Duration::from_millis(150), async {
        loop {
            match app_events.recv().await {
                Ok(AppEvent::Fade {
                    event: FadeEvent::FadeCompleted,
                    ..
                }) => return true,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        completed,
        "zero-duration pan-only fade should emit FadeCompleted"
    );
}

#[tokio::test]
async fn zero_duration_pan_family_targets_send_exact_final_values() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&lv1_frame("/Channels", &two_channels_args()))
            .unwrap();

        let mut buf = [0_u8; 1024];
        let mut decoder = TestFrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]) {
                        let msg = decode_frame_payload(&frame).unwrap();
                        match msg.address.as_str() {
                            "/Set/Track/Pan"
                            | "/Set/Track/Pan/Balance"
                            | "/Set/Track/Pan/Width" => {
                                if let (
                                    Some(OscArg::Int(group)),
                                    Some(OscArg::Int(channel)),
                                    Some(OscArg::Double(value)),
                                ) = (msg.args.first(), msg.args.get(1), msg.args.get(2))
                                {
                                    let _ =
                                        tx.send((msg.address.clone(), *group, *channel, *value));
                                }
                            }
                            _ => {}
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![
                FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::Pan,
                    target: -12.0,
                },
                FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::Balance,
                    target: 7.5,
                },
                FadeTarget {
                    group: 0,
                    channel: 1,
                    parameter: FadeParameter::Width,
                    target: 1.4,
                },
            ],
            0,
        ),
    )
    .await
    .unwrap();

    let mut sent = Vec::new();
    for _ in 0..3 {
        sent.push(rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap());
    }

    assert!(sent.contains(&("/Set/Track/Pan".to_string(), 0, 0, -12.0)));
    assert!(sent.contains(&("/Set/Track/Pan/Balance".to_string(), 0, 0, 7.5)));
    assert!(sent.contains(&("/Set/Track/Pan/Width".to_string(), 0, 1, 1.4)));
}

#[tokio::test]
async fn pan_family_override_cancels_pan_targets_without_stopping_fader() {
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
        std::thread::sleep(std::time::Duration::from_millis(400));
        stream
            .write_all(&lv1_frame(
                "/Notify/Track/Pan",
                &[
                    OscArg::Int(0),
                    OscArg::Int(0),
                    OscArg::Double(45.0),
                    OscArg::True,
                ],
            ))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(25));
        stream
            .write_all(&lv1_frame(
                "/Notify/Track/Pan",
                &[
                    OscArg::Int(0),
                    OscArg::Int(0),
                    OscArg::Double(45.0),
                    OscArg::True,
                ],
            ))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(3));
    });

    let event_bus = AppEventBus::default();
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![
                FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -20.0,
                },
                FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::Pan,
                    target: -12.0,
                },
            ],
            10_000,
        ),
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(
            e,
            FadeEvent::ChannelOverride {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
            }
        )
    })
    .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(
            e,
            FadeEvent::ChannelCancelled {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
            }
        )
    })
    .await;

    let still_running = tokio::time::timeout(std::time::Duration::from_millis(400), async {
        loop {
            match app_events.recv().await {
                Ok(AppEvent::Fade {
                    event: FadeEvent::FadeCompleted,
                    ..
                }) => return false,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(true);

    assert!(
        still_running,
        "fader fade should continue after pan-family override"
    );
}

#[tokio::test]
async fn fader_override_keeps_pan_family_targets_active_for_same_channel() {
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

        let mut buf = [0_u8; 1024];
        let mut decoder = TestFrameDecoder::default();
        let mut sent_override = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]) {
                        let msg = decode_frame_payload(&frame).unwrap();
                        if !sent_override && msg.address == "/Set/Track/Pan" {
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
                            sent_override = true;
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    wait_for_app_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(
            e,
            AppEvent::Lv1 {
                event: Lv1Event::ChannelTopologyChanged(_),
                ..
            }
        )
    })
    .await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![
                FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::FaderDb,
                    target: -20.0,
                },
                FadeTarget {
                    group: 0,
                    channel: 0,
                    parameter: FadeParameter::Pan,
                    target: -12.0,
                },
            ],
            10_000,
        ),
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(
            e,
            FadeEvent::ChannelOverride {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
            }
        )
    })
    .await;

    let pan_cancelled = tokio::time::timeout(std::time::Duration::from_millis(200), async {
        loop {
            match app_events.recv().await {
                Ok(AppEvent::Fade {
                    event:
                        FadeEvent::ChannelCancelled {
                            group,
                            channel,
                            parameter,
                        },
                    ..
                }) if group == 0 && channel == 0 && parameter == FadeParameter::Pan => {
                    return true;
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(!pan_cancelled, "pan-family target must not be canceled");
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let intro = scene(1, "Intro");
    start_fade(
        &engine,
        fade_config(
            intro.clone(),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -12.0,
            }],
            1_000,
        ),
    )
    .await
    .unwrap();

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |event| matches!(event, FadeEvent::FadeStarted),
    )
    .await;

    start_fade(
        &engine,
        fade_config(
            intro,
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
                target: -12.0,
            }],
            0,
        ),
    )
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        FadeConfig {
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
        },
    )
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        FadeConfig {
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
        },
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = abort_all(&engine).await;

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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        FadeConfig {
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
        },
    )
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        FadeConfig {
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
        },
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = start_fade(
        &engine,
        FadeConfig {
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
        },
    )
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            30_000,
        ),
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(2, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            500,
        ),
    )
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
                channel: 1,
                parameter: FadeParameter::FaderDb,
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            30_000,
        ),
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(2, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            30_000,
        ),
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            30_000,
        ),
    )
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
        let mut decoder = TestFrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]) {
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        fade_config(
            scene(1, "Intro"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -30.0,
            }],
            1_000,
        ),
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    while gain_rx.try_recv().is_ok() {}

    let _ = start_fade(
        &engine,
        fade_config(
            scene(2, "Verse"),
            vec![FadeTarget {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
                target: -10.0,
            }],
            1_000,
        ),
    )
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

#[tokio::test]
async fn disconnect_aborts_active_fade() {
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
        std::thread::sleep(std::time::Duration::from_millis(300));
        // Connection drops by closing the stream
    });

    let event_bus = AppEventBus::default();
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        FadeConfig {
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
        },
    )
    .await;

    wait_for_app_fade_event(
        &mut app_events,
        std::time::Duration::from_millis(500),
        |e| matches!(e, FadeEvent::FadeStarted),
    )
    .await;

    // Let the server's disconnect happen and propagate
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(2), |e| {
        matches!(e, FadeEvent::FadeAborted)
    })
    .await;

    // Verify no FadeCompleted event after abort
    let completed = tokio::time::timeout(std::time::Duration::from_millis(250), async {
        loop {
            match app_events.recv().await {
                Ok(AppEvent::Fade {
                    event: FadeEvent::FadeCompleted,
                    ..
                }) => return true,
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
        "FadeCompleted should not be emitted after disconnect aborts fade"
    );
}

#[tokio::test]
async fn override_of_last_target_emits_terminal_event() {
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
    let lv1 = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let (_command_bus, engine) = spawn_runtime_for_test(lv1.clone(), event_bus.clone()).await;
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = start_fade(
        &engine,
        FadeConfig {
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
        },
    )
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

    wait_for_app_fade_event(&mut app_events, std::time::Duration::from_secs(3), |e| {
        matches!(e, FadeEvent::FadeCompleted)
    })
    .await;
}

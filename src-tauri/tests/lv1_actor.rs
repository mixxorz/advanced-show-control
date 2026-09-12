use advanced_show_control::lv1::osc::OscArg;
use advanced_show_control::lv1::{
    ConnectionStatus, Lv1Command, Lv1Event, Lv1Frame, SceneObservation, SceneState, build_actor,
    decode_frame_payload, encode_frame,
};
use advanced_show_control::runtime::events::{AppEvent, AppEventBus};
use std::io::Write;
use std::net::TcpListener;
use tokio::sync::oneshot;

fn make_lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
    encode_frame(address, args).unwrap()
}

fn build_and_spawn_actor(
    host: String,
    port: u16,
    event_bus: AppEventBus,
    generation: u64,
) -> advanced_show_control::lv1::Lv1ActorHandle {
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

async fn wait_for_connected(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1 {
                    event: Lv1Event::Connected,
                    ..
                }) => return,
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("event stream closed before Connected")
                }
            }
        }
    })
    .await
    .expect("timed out waiting for Connected");
}

async fn recv_scene_observation(
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
) -> SceneObservation {
    loop {
        if let AppEvent::Lv1 {
            event: Lv1Event::SceneChanged(observation),
            ..
        } = events.recv().await.unwrap()
        {
            return observation;
        }
    }
}

#[tokio::test]
async fn actor_connects_and_emits_connected_event() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = tokio::task::spawn_blocking(move || {
        let (_stream, _) = listener.accept().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(
        event,
        AppEvent::Lv1 {
            event: Lv1Event::Connected,
            ..
        }
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn actor_emits_disconnected_and_reconnects_when_server_closes() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    let _server = tokio::task::spawn_blocking(move || {
        for i in 0..2 {
            match listener.accept() {
                Ok((stream, _)) => {
                    if i == 0 {
                        drop(stream);
                    } else {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                }
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    break;
                }
            }
        }
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    let mut got_disconnect = false;
    let mut got_reconnect = false;
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Ok(event) = events.recv().await {
            match event {
                AppEvent::Lv1 {
                    event: Lv1Event::Disconnected { .. },
                    ..
                } => got_disconnect = true,
                AppEvent::Lv1 {
                    event: Lv1Event::Connected,
                    ..
                } if got_disconnect => {
                    got_reconnect = true;
                    break;
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(result.is_ok(), "timed out waiting for reconnect");
    assert!(got_reconnect);
}

#[tokio::test]
async fn actor_publishes_monotonic_scene_observations_and_recall_dispatch_boundary() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (send_second_scene_tx, send_second_scene_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        stream
            .write_all(&make_lv1_frame("/Notify/CurSceneIndex", &[OscArg::Int(3)]))
            .unwrap();
        stream
            .write_all(&make_lv1_frame(
                "/Notify/Scene/Name",
                &[OscArg::String("Bridge".to_string())],
            ))
            .unwrap();

        send_second_scene_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();

        use std::io::Read;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut decoder = TestFrameDecoder::default();
        let mut buffer = [0_u8; 1024];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            let recall = decoder
                .push(&buffer[..count])
                .into_iter()
                .map(|frame| decode_frame_payload(&frame).unwrap())
                .find(|message| message.address == "/Set/CurSceneIndex");
            if let Some(recall) = recall {
                assert_eq!(recall.args, vec![OscArg::Int(1)]);
                break;
            }
        }

        stream
            .write_all(&make_lv1_frame(
                "/Notify/Scene/Name",
                &[OscArg::String("Chorus".to_string())],
            ))
            .unwrap();
        stream
            .write_all(&make_lv1_frame("/Notify/CurSceneIndex", &[OscArg::Int(4)]))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    let first = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        recv_scene_observation(&mut events),
    )
    .await
    .unwrap();
    assert_eq!(
        first,
        SceneObservation {
            sequence: 1,
            scene: SceneState {
                index: 3,
                name: "Bridge".to_string(),
            },
        }
    );

    let (reply, rx) = oneshot::channel();
    _handle
        .send(Lv1Command::RecallScene {
            scene_index: 1,
            reply: Some(reply),
        })
        .await
        .unwrap();
    let dispatch = rx.await.unwrap().unwrap();
    assert_eq!(dispatch.scene_observation_sequence, 1);

    send_second_scene_tx.send(()).unwrap();
    let second = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        recv_scene_observation(&mut events),
    )
    .await
    .unwrap();
    assert_eq!(
        second,
        SceneObservation {
            sequence: 2,
            scene: SceneState {
                index: 4,
                name: "Chorus".to_string(),
            },
        }
    );
}

#[tokio::test]
async fn actor_resets_scene_observation_sequence_after_reconnecting() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (release_first_peer_tx, release_first_peer_rx) = std::sync::mpsc::channel();
    let (release_second_peer_tx, release_second_peer_rx) = std::sync::mpsc::channel();

    let server = tokio::task::spawn_blocking(move || {
        for (connection_index, (index, name)) in
            [(3, "Bridge"), (4, "Chorus")].into_iter().enumerate()
        {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            stream
                .write_all(&make_lv1_frame(
                    "/Notify/CurSceneIndex",
                    &[OscArg::Int(index)],
                ))
                .unwrap();
            stream
                .write_all(&make_lv1_frame(
                    "/Notify/Scene/Name",
                    &[OscArg::String(name.to_string())],
                ))
                .unwrap();

            match connection_index {
                0 => release_first_peer_rx
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap(),
                1 => release_second_peer_rx
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap(),
                _ => unreachable!(),
            }
        }
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    let first = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        recv_scene_observation(&mut events),
    )
    .await
    .expect("actor did not publish the first scene observation");
    assert_eq!(
        first,
        SceneObservation {
            sequence: 1,
            scene: SceneState {
                index: 3,
                name: "Bridge".to_string(),
            },
        }
    );
    release_first_peer_tx.send(()).unwrap();
    let second = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        recv_scene_observation(&mut events),
    )
    .await
    .expect("actor did not publish the scene observation after reconnect");
    assert_eq!(
        second,
        SceneObservation {
            sequence: 1,
            scene: SceneState {
                index: 4,
                name: "Chorus".to_string(),
            },
        }
    );
    release_second_peer_tx.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn get_state_returns_snapshot_with_current_values() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        let mut channel = vec![
            OscArg::Int(1),
            OscArg::String("Stereo 1".to_string()),
            OscArg::Int(0),
            OscArg::Int(0),
            OscArg::Double(-8.0),
            OscArg::Double(0.0),
        ];
        channel.extend((0..11).map(|_| OscArg::Int(0)));
        channel.push(OscArg::Int(2));
        channel.push(OscArg::Int64(0));
        channel.push(OscArg::Double(0.0));
        for (address, args) in [
            ("/Channels", channel),
            (
                "/Notify/SceneList",
                vec![
                    OscArg::Int(1),
                    OscArg::Int(4),
                    OscArg::String("Outro".to_string()),
                ],
            ),
            (
                "/Notify/Track/Out/Gain",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Double(-6.0)],
            ),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Bool(true)],
            ),
            (
                "/Notify/Track/Pan",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Double(-15.0)],
            ),
            (
                "/Notify/Balance",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Double(0.25)],
            ),
            (
                "/Notify/PanArcWidth",
                vec![
                    OscArg::Int(0),
                    OscArg::Int(0),
                    OscArg::Double(1.2),
                    OscArg::Int(1),
                ],
            ),
            ("/Notify/CurSceneIndex", vec![OscArg::Int(4)]),
            (
                "/Notify/Scene/Name",
                vec![OscArg::String("Outro".to_string())],
            ),
        ] {
            stream.write_all(&make_lv1_frame(address, &args)).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                AppEvent::Lv1 {
                    event: Lv1Event::SceneChanged(_),
                    ..
                }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();

    let (reply, rx) = oneshot::channel();
    handle.send(Lv1Command::GetState { reply }).await.unwrap();
    let snapshot = rx.await.unwrap();
    assert_eq!(snapshot.connection, ConnectionStatus::Connected);
    assert_eq!(
        snapshot.scene,
        Some(SceneState {
            index: 4,
            name: "Outro".to_string()
        })
    );
    assert_eq!(snapshot.scene_list.len(), 1);
    assert_eq!(snapshot.channels.len(), 1);
    let channel = &snapshot.channels[0];
    assert_eq!((channel.gain_db, channel.muted), (-6.0, true));
    assert_eq!(
        (channel.pan, channel.balance, channel.width),
        (Some(-15.0), Some(0.25), Some(1.2))
    );
}

#[tokio::test]
async fn actor_ignores_invalid_or_inapplicable_channel_updates() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();

        let mut channels = vec![OscArg::Int(2)];
        for (name, channel, pan_mode) in [("Stereo", 0, 2), ("Mono", 1, 1)] {
            channels.extend([
                OscArg::String(name.to_string()),
                OscArg::Int(0),
                OscArg::Int(channel),
                OscArg::Double(-8.0),
                OscArg::Double(0.0),
            ]);
            channels.extend((0..11).map(|_| OscArg::Int(0)));
            channels.extend([OscArg::Int(pan_mode), OscArg::Int64(0), OscArg::Double(0.0)]);
        }

        let script = [
            ("/Channels", channels),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Int(0)],
            ),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Int(1)],
            ),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Bool(true)],
            ),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Bool(false)],
            ),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Int(2)],
            ),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(0), OscArg::Int(-1)],
            ),
            (
                "/Notify/PanArcWidth",
                vec![
                    OscArg::Int(0),
                    OscArg::Int(0),
                    OscArg::Double(1.2),
                    OscArg::Int(0),
                ],
            ),
            (
                "/Notify/Balance",
                vec![OscArg::Int(0), OscArg::Int(1), OscArg::Double(0.75)],
            ),
            (
                "/Notify/Track/Out/Gain",
                vec![OscArg::Int(0), OscArg::Int(99), OscArg::Double(-3.0)],
            ),
            (
                "/Notify/Track/Out/Mute",
                vec![OscArg::Int(0), OscArg::Int(99), OscArg::Bool(true)],
            ),
            (
                "/Notify/Track/Pan",
                vec![OscArg::Int(0), OscArg::Int(99), OscArg::Double(30.0)],
            ),
            (
                "/Notify/Scene/Name",
                vec![OscArg::String("Updates complete".to_string())],
            ),
            ("/Notify/CurSceneIndex", vec![OscArg::Int(9)]),
        ];
        for (address, args) in script {
            stream.write_all(&make_lv1_frame(address, &args)).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_secs(3));
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);
    let mute_facts = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut mute_facts = Vec::new();
        loop {
            match events.recv().await.unwrap() {
                AppEvent::Lv1 {
                    event:
                        Lv1Event::MuteChanged {
                            group: 0,
                            channel: 0,
                            muted,
                        },
                    ..
                } => mute_facts.push(muted),
                AppEvent::Lv1 {
                    event: Lv1Event::SceneChanged(_),
                    ..
                } => break mute_facts,
                _ => {}
            }
        }
    })
    .await
    .expect("actor did not finish the ordered update script");
    assert_eq!(
        mute_facts,
        vec![false, true, true, false],
        "invalid integer mute reports must publish no facts"
    );

    let (reply, snapshot) = oneshot::channel();
    handle.send(Lv1Command::GetState { reply }).await.unwrap();
    let snapshot = snapshot.await.unwrap();
    assert_eq!(snapshot.channels.len(), 2);
    assert!(
        !snapshot.channels[0].muted,
        "invalid integer mute reports must not change the final valid false value"
    );
    assert_eq!(
        snapshot.channels[0].width, None,
        "inactive width must be ignored"
    );
    assert_eq!(
        snapshot.channels[1].balance, None,
        "mono balance must be ignored"
    );
    assert_eq!(
        snapshot.channels[0].gain_db, -8.0,
        "unknown-channel gain must not affect known channels"
    );
    assert_eq!(
        snapshot.channels[0].pan,
        Some(0.0),
        "unknown-channel pan must not affect known channels"
    );
}

#[tokio::test]
async fn actor_sends_set_gain_while_waiting_for_input() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (address_tx, address_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        use std::io::Read;

        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
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
                        let _ = address_tx.send(msg.address);
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
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    wait_for_connected(&mut events).await;

    let sent_at = std::time::Instant::now();
    let (reply, rx) = oneshot::channel();
    assert!(
        handle
            .send(Lv1Command::SetGain {
                group: 0,
                channel: 1,
                gain_db: -12.5,
                reply: Some(reply),
            })
            .await
            .is_ok()
    );
    assert!(rx.await.unwrap().is_ok());

    tokio::task::spawn_blocking(move || {
        loop {
            let address = address_rx
                .recv_timeout(std::time::Duration::from_millis(150))
                .expect("SetGain frame was not sent promptly while actor was waiting for input");
            if address == "/Set/Track/Out/Gain" {
                assert!(sent_at.elapsed() < std::time::Duration::from_millis(150));
                break;
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn actor_publishes_ping_sequence_facts_after_routing_pongs() {
    use std::io::Read;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (pong_tx, pong_rx) = std::sync::mpsc::channel();
    let first_ping_args = vec![OscArg::Int64(42)];
    let second_ping_args = vec![OscArg::String("ready".to_string()), OscArg::Int(7)];
    let first_server_ping_args = first_ping_args.clone();
    let second_server_ping_args = second_ping_args.clone();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&make_lv1_frame("/ping", &first_server_ping_args))
            .unwrap();
        stream
            .write_all(&make_lv1_frame("/ping", &second_server_ping_args))
            .unwrap();

        let mut buf = [0_u8; 1024];
        let mut decoder = TestFrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut pong_count = 0;
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]) {
                        let msg = decode_frame_payload(&frame).unwrap();
                        if msg.address == "/pong" {
                            pong_tx.send(msg.args).unwrap();
                            pong_count += 1;
                        }
                    }
                    if pong_count == 2 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        break;
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
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 7);

    let (first_pong_args, second_pong_args) = tokio::task::spawn_blocking(move || {
        (
            pong_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            pong_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
        )
    })
    .await
    .unwrap();

    let first_ping_event = loop {
        let event = events.recv().await.unwrap();
        if matches!(
            event,
            AppEvent::Lv1 {
                event: Lv1Event::PingReceived { .. },
                ..
            }
        ) {
            break event;
        }
    };
    let second_ping_event = loop {
        let event = events.recv().await.unwrap();
        if matches!(
            event,
            AppEvent::Lv1 {
                event: Lv1Event::PingReceived { .. },
                ..
            }
        ) {
            break event;
        }
    };

    let (reply, rx) = oneshot::channel();
    handle.send(Lv1Command::GetState { reply }).await.unwrap();
    let snapshot = rx.await.unwrap();

    assert!(matches!(
        first_ping_event,
        AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 1 },
        }
    ));
    assert!(matches!(
        second_ping_event,
        AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::PingReceived { sequence: 2 },
        }
    ));
    assert_eq!(snapshot.ping_sequence, 2);
    assert_eq!(first_pong_args, first_ping_args);
    assert_eq!(second_pong_args, second_ping_args);
}

#[tokio::test]
async fn actor_resets_ping_sequence_after_reconnecting() {
    use std::io::Read;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (release_server_tx, release_server_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        for (connection_index, ping_args) in [vec![OscArg::Int(1)], vec![OscArg::Int(2)]]
            .into_iter()
            .enumerate()
        {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_millis(50)))
                .unwrap();
            stream
                .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
                .unwrap();
            stream
                .write_all(&make_lv1_frame("/ping", &ping_args))
                .unwrap();

            let mut buffer = [0_u8; 1024];
            let mut decoder = TestFrameDecoder::default();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            'wait_for_pong: loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        for frame in decoder.push(&buffer[..count]) {
                            let message = decode_frame_payload(&frame).unwrap();
                            if message.address == "/pong" {
                                assert_eq!(message.args, ping_args);
                                break 'wait_for_pong;
                            }
                        }
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(error) => panic!("server read failed: {error}"),
                }

                if std::time::Instant::now() >= deadline {
                    panic!("server did not receive pong");
                }
            }

            if connection_index == 1 {
                release_server_rx
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            }
        }
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 7);
    let mut sequences = Vec::new();

    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        while sequences.len() < 2 {
            if let AppEvent::Lv1 {
                event: Lv1Event::PingReceived { sequence },
                ..
            } = events.recv().await.unwrap()
            {
                sequences.push(sequence);
            }
        }
    })
    .await
    .expect("actor did not accept pings across reconnect");

    let (reply, rx) = oneshot::channel();
    handle.send(Lv1Command::GetState { reply }).await.unwrap();
    let snapshot = rx.await.unwrap();
    release_server_tx.send(()).unwrap();

    assert_eq!(sequences, vec![1, 1]);
    assert_eq!(snapshot.ping_sequence, 1);
}

#[tokio::test]
async fn disconnected_flush_and_recall_scene_return_errors() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, AppEventBus::default(), 0);

    let (flush_reply, flush_result) = oneshot::channel();
    handle
        .send(Lv1Command::Flush {
            reply: Some(flush_reply),
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), flush_result)
            .await
            .expect("disconnected Flush reply timed out")
            .unwrap()
            .is_err()
    );

    let (recall_reply, recall_result) = oneshot::channel();
    handle
        .send(Lv1Command::RecallScene {
            scene_index: 4,
            reply: Some(recall_reply),
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(2), recall_result)
            .await
            .expect("disconnected RecallScene reply timed out")
            .unwrap()
            .is_err()
    );
}

#[tokio::test]
async fn actor_set_mute_returns_error_when_connection_drops_before_ack() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        drop(stream);
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    wait_for_connected(&mut events).await;

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1 {
                    event: Lv1Event::Disconnected { .. },
                    ..
                }) => return,
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("event stream closed before Disconnected")
                }
            }
        }
    })
    .await
    .expect("timed out waiting for Disconnected");

    let (reply, rx) = oneshot::channel();
    let send_result = handle
        .send(Lv1Command::SetMute {
            group: 0,
            channel: 1,
            muted: true,
            reply: Some(reply),
        })
        .await;
    if send_result.is_ok() {
        assert!(rx.await.unwrap().is_err());
    }
}

#[tokio::test]
async fn actor_flush_waits_for_successful_gain_and_mute_writes() {
    enum WriteCase {
        Gain { value: f64 },
        Mute { value: bool },
    }

    for case in [
        WriteCase::Gain { value: -9.5 },
        WriteCase::Mute { value: true },
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (message_tx, message_rx) = std::sync::mpsc::channel();

        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_millis(50)))
                .unwrap();
            let mut buffer = [0_u8; 1024];
            let mut decoder = TestFrameDecoder::default();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        for frame in decoder.push(&buffer[..count]) {
                            let message = decode_frame_payload(&frame).unwrap();
                            if matches!(
                                message.address.as_str(),
                                "/Set/Track/Out/Gain" | "/Set/Track/Out/Mute"
                            ) {
                                message_tx.send(message).unwrap();
                                return;
                            }
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => panic!("server read failed: {error}"),
                }
            }
            panic!("parameter write was not received");
        });

        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);
        wait_for_connected(&mut events).await;

        let (write_reply, write_result) = oneshot::channel();
        let (expected_address, expected_args) = match case {
            WriteCase::Gain { value } => {
                handle
                    .send(Lv1Command::SetGain {
                        group: 0,
                        channel: 1,
                        gain_db: value,
                        reply: Some(write_reply),
                    })
                    .await
                    .unwrap();
                (
                    "/Set/Track/Out/Gain",
                    vec![OscArg::Int(0), OscArg::Int(1), OscArg::Double(value)],
                )
            }
            WriteCase::Mute { value } => {
                handle
                    .send(Lv1Command::SetMute {
                        group: 0,
                        channel: 1,
                        muted: value,
                        reply: Some(write_reply),
                    })
                    .await
                    .unwrap();
                (
                    "/Set/Track/Out/Mute",
                    vec![OscArg::Int(0), OscArg::Int(1), OscArg::Bool(value)],
                )
            }
        };
        assert_eq!(write_result.await.unwrap(), Ok(()));

        let (flush_reply, flush_result) = oneshot::channel();
        handle
            .send(Lv1Command::Flush {
                reply: Some(flush_reply),
            })
            .await
            .unwrap();
        assert_eq!(flush_result.await.unwrap(), Ok(()));

        let message = tokio::task::spawn_blocking(move || {
            message_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("write frame was not received before flush returned")
        })
        .await
        .unwrap();
        assert_eq!(message.address, expected_address);
        assert_eq!(message.args, expected_args);
    }
}

#[tokio::test(start_paused = true)]
async fn silent_server_disconnects_after_ping_timeout() {
    use std::io::Write;
    use std::sync::mpsc as std_mpsc;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    // Channel to signal the server to stay alive until we're done
    let (done_tx, done_rx) = std_mpsc::channel::<()>();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();

        // Send handshake so actor reaches Connected state
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();

        // Hold the connection open (go silent — no pings, no data)
        // until the test signals we're done
        let _ = done_rx.recv();
        drop(stream);
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    // With paused time, advance a little to let the TCP handshake complete
    tokio::time::advance(std::time::Duration::from_millis(100)).await;
    tokio::task::yield_now().await;

    // Wait for Connected event (no timeout needed — TCP I/O drives this)
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Ok(event) = events.recv().await {
            if matches!(
                event,
                AppEvent::Lv1 {
                    event: Lv1Event::Connected,
                    ..
                }
            ) {
                return;
            }
        }
    })
    .await
    .expect("actor did not connect");

    // Advance time past PING_TIMEOUT (10 seconds) — this fires the sleep_until branch
    // in the connected loop select!, causing PingTimeout disconnect
    tokio::time::advance(std::time::Duration::from_secs(11)).await;
    tokio::task::yield_now().await;

    // Assert Disconnected is published and names the ping timeout as the reason
    let disconnect_reason = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Ok(event) = events.recv().await {
            if let AppEvent::Lv1 {
                event: Lv1Event::Disconnected { reason },
                ..
            } = event
            {
                return Some(reason);
            }
        }
        None
    })
    .await
    .expect("timed out waiting for Disconnected after advancing past PING_TIMEOUT");

    let reason = disconnect_reason.expect("Disconnected event not received after ping timeout");
    assert!(
        reason.contains("ping timeout"),
        "disconnect reason should name the ping timeout, got: {reason}"
    );
    let _ = done_tx.send(());
}

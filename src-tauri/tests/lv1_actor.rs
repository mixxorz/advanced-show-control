use advanced_show_control::lv1::osc::OscArg;
use advanced_show_control::lv1::{
    ConnectionStatus, Lv1Command, Lv1Event, Lv1Frame, build_actor, decode_frame_payload,
    encode_frame,
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
    .unwrap();
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
async fn actor_parses_and_emits_scene_changed() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        stream
            .write_all(&make_lv1_frame(
                "/Notify/Scene/Name",
                &[OscArg::String("Scene A".to_string())],
            ))
            .unwrap();
        stream
            .write_all(&make_lv1_frame("/Notify/CurSceneIndex", &[OscArg::Int(0)]))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    let mut scene_event = None;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Ok(event) = events.recv().await {
            if let AppEvent::Lv1 {
                event: Lv1Event::SceneChanged(s),
                ..
            } = event
            {
                scene_event = Some(s);
                break;
            }
        }
    })
    .await
    .unwrap();

    let scene = scene_event.unwrap();
    assert_eq!(scene.index, 0);
    assert_eq!(scene.name, "Scene A");
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
        std::thread::sleep(std::time::Duration::from_millis(500));
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    wait_for_connected(&mut events).await;

    let (reply, rx) = oneshot::channel();
    handle.send(Lv1Command::GetState { reply }).await.unwrap();
    let snapshot = rx.await.unwrap();
    assert_eq!(snapshot.connection, ConnectionStatus::Connected);
}

#[tokio::test]
async fn actor_handles_set_gain_command() {
    use std::io::Read;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut buf = [0u8; 4096];
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(500)))
            .unwrap();
        let _ = stream.read(&mut buf);

        std::thread::sleep(std::time::Duration::from_millis(500));
    });

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);

    wait_for_connected(&mut events).await;

    let (reply, rx) = oneshot::channel();
    assert!(
        handle
            .send(Lv1Command::SetGain {
                group: 0,
                channel: 0,
                gain_db: -20.0,
                reply: Some(reply),
            })
            .await
            .is_ok()
    );
    assert!(rx.await.unwrap().is_ok());
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
async fn actor_sends_set_mute_while_waiting_for_input() {
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
            .send(Lv1Command::SetMute {
                group: 0,
                channel: 1,
                muted: true,
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
                .expect("SetMute frame was not sent promptly while actor was waiting for input");
            if address == "/Set/Track/Out/Mute" {
                assert!(sent_at.elapsed() < std::time::Duration::from_millis(150));
                break;
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn actor_routes_pong_without_blocking_read_loop() {
    use std::io::Read;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (pong_tx, pong_rx) = std::sync::mpsc::channel();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        stream
            .write_all(&make_lv1_frame("/handshake", &[OscArg::Int(1)]))
            .unwrap();
        stream
            .write_all(&make_lv1_frame("/ping", &[OscArg::Int64(42)]))
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
                        if msg.address == "/pong" {
                            pong_tx.send(msg.args).unwrap();
                            return;
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
    let mut events = event_bus.subscribe();
    let _handle = build_and_spawn_actor("127.0.0.1".to_string(), port, event_bus, 0);
    wait_for_connected(&mut events).await;

    let args = tokio::task::spawn_blocking(move || {
        pong_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap()
    })
    .await
    .unwrap();

    assert_eq!(args, vec![OscArg::Int64(42)]);
}

#[tokio::test]
async fn actor_set_mute_returns_error_when_actor_is_unavailable() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let handle = build_and_spawn_actor("127.0.0.1".to_string(), port, AppEventBus::default(), 0);

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let (reply, rx) = oneshot::channel();
        handle
            .send(Lv1Command::SetMute {
                group: 0,
                channel: 1,
                muted: true,
                reply: Some(reply),
            })
            .await?;
        rx.await.unwrap()
    })
    .await
    .unwrap();

    assert!(result.is_err());
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
        while let Ok(event) = events.recv().await {
            if matches!(
                event,
                AppEvent::Lv1 {
                    event: Lv1Event::Disconnected { .. },
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
async fn actor_flush_waits_for_prior_set_mute_command() {
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

    let (reply, rx) = oneshot::channel();
    assert!(
        handle
            .send(Lv1Command::SetMute {
                group: 0,
                channel: 1,
                muted: true,
                reply: Some(reply),
            })
            .await
            .is_ok()
    );
    assert!(rx.await.unwrap().is_ok());
    let (reply, rx) = oneshot::channel();
    assert!(
        handle
            .send(Lv1Command::Flush { reply: Some(reply) })
            .await
            .is_ok()
    );
    assert!(rx.await.unwrap().is_ok());

    loop {
        let address = address_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("SetMute frame was not sent before flush returned");
        if address == "/Set/Track/Out/Mute" {
            break;
        }
    }
}

#[tokio::test]
async fn actor_flush_waits_for_prior_set_gain_command() {
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

    let (reply, rx) = oneshot::channel();
    assert!(
        handle
            .send(Lv1Command::SetGain {
                group: 0,
                channel: 1,
                gain_db: -9.5,
                reply: Some(reply),
            })
            .await
            .is_ok()
    );
    assert!(rx.await.unwrap().is_ok());
    let (reply, rx) = oneshot::channel();
    assert!(
        handle
            .send(Lv1Command::Flush { reply: Some(reply) })
            .await
            .is_ok()
    );
    assert!(rx.await.unwrap().is_ok());

    loop {
        let address = address_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("SetGain frame was not sent before flush returned");
        if address == "/Set/Track/Out/Gain" {
            break;
        }
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

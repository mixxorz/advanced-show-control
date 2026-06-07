use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::ConnectionStatus;
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use lv1_scene_fade_utility::lv1::tcp::{FrameDecoder, decode_frame_payload, encode_frame};
use lv1_scene_fade_utility::osc::OscArg;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus};
use std::io::Write;
use std::net::TcpListener;

fn make_lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
    encode_frame(address, args).unwrap()
}

async fn wait_for_connected(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Ok(event) = events.recv().await {
            if matches!(event, AppEvent::Lv1(Lv1Event::Connected)) {
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
    let _handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(event, AppEvent::Lv1(Lv1Event::Connected)));
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
    let _handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    let mut got_disconnect = false;
    let mut got_reconnect = false;
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Ok(event) = events.recv().await {
            match event {
                AppEvent::Lv1(Lv1Event::Disconnected) => got_disconnect = true,
                AppEvent::Lv1(Lv1Event::Connected) if got_disconnect => {
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
    let _handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    let mut scene_event = None;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while let Ok(event) = events.recv().await {
            if let AppEvent::Lv1(Lv1Event::SceneChanged(s)) = event {
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
    let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    wait_for_connected(&mut events).await;

    let snapshot = handle.get_state().await;
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
    let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    wait_for_connected(&mut events).await;

    assert!(handle.set_gain(0, 0, -20.0).await.is_ok());
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
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
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
    let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    wait_for_connected(&mut events).await;

    let sent_at = std::time::Instant::now();
    assert!(handle.set_gain(0, 1, -12.5).await.is_ok());

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
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
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
    let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    wait_for_connected(&mut events).await;

    let sent_at = std::time::Instant::now();
    assert!(handle.set_mute(0, 1, true).await.is_ok());

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
async fn actor_set_mute_returns_error_when_actor_is_unavailable() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let handle = spawn_actor("127.0.0.1".to_string(), port, AppEventBus::default());

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        handle.set_mute(0, 1, true),
    )
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
    let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    wait_for_connected(&mut events).await;

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Ok(event) = events.recv().await {
            if matches!(event, AppEvent::Lv1(Lv1Event::Disconnected)) {
                break;
            }
        }
    })
    .await
    .unwrap();

    let result = handle.set_mute(0, 1, true).await;
    assert!(result.is_err());
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
        let mut decoder = FrameDecoder::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]).unwrap() {
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
    let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus);

    wait_for_connected(&mut events).await;

    assert!(handle.set_mute(0, 1, true).await.is_ok());
    assert!(handle.flush().await.is_ok());

    loop {
        let address = address_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("SetMute frame was not sent before flush returned");
        if address == "/Set/Track/Out/Mute" {
            break;
        }
    }
}

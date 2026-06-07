use lv1_scene_fade_utility::fade::curve::FadeCurve;
use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::fade::types::{FadeConfig, FadeEvent, FadeTarget};
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use lv1_scene_fade_utility::lv1::tcp::encode_frame;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::dispatcher::RuntimeDispatcher;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus};
use lv1_scene_fade_utility::osc::OscArg;
use std::io::Write;
use std::net::TcpListener;
use tokio::sync::mpsc;

fn lv1_frame(address: &str, args: &[OscArg]) -> Vec<u8> {
    encode_frame(address, args).unwrap()
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

fn spawn_runtime_for_test(
    lv1: lv1_scene_fade_utility::lv1::state::Lv1ActorHandle,
    event_bus: AppEventBus,
) -> AppCommandBus {
    let (tx, rx) = mpsc::channel(32);
    let bus = AppCommandBus::new(tx);
    let mut dispatcher = RuntimeDispatcher::new(rx, event_bus);
    dispatcher.set_lv1(Some(lv1));
    tokio::spawn(async move { dispatcher.run().await });
    bus
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
    let command_bus = spawn_runtime_for_test(lv1.clone(), event_bus.clone());
    let engine = spawn_engine(command_bus, event_bus.clone());
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                target_db: -10.0,
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
    let command_bus = spawn_runtime_for_test(lv1.clone(), event_bus.clone());
    let engine = spawn_engine(command_bus, event_bus.clone());
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                target_db: -30.0,
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
    let command_bus = spawn_runtime_for_test(lv1.clone(), event_bus.clone());
    let engine = spawn_engine(command_bus, event_bus.clone());
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                target_db: -20.0,
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
async fn start_fade_while_running_replaces_previous() {
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
    let command_bus = spawn_runtime_for_test(lv1.clone(), event_bus.clone());
    let engine = spawn_engine(command_bus, event_bus.clone());
    let mut app_events = event_bus.subscribe();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let _ = engine
        .start_fade(FadeConfig {
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                target_db: -30.0,
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
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                target_db: -10.0,
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

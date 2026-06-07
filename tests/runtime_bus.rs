use lv1_scene_fade_utility::fade::curve::FadeCurve;
use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::fade::types::{FadeConfig, FadeTarget};
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use lv1_scene_fade_utility::lv1::tcp::encode_frame;
use lv1_scene_fade_utility::osc::OscArg;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::dispatcher::RuntimeDispatcher;
use lv1_scene_fade_utility::runtime::events::AppEventBus;
use std::io::Write;
use std::net::TcpListener;
use tokio::sync::mpsc;

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

#[tokio::test]
async fn routed_start_fade_completes_when_fade_queries_lv1_state() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::task::spawn_blocking(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(&encode_frame("/handshake", &[OscArg::Int(1)]).unwrap())
            .unwrap();
        stream
            .write_all(&encode_frame("/Channels", &channels_args()).unwrap())
            .unwrap();
        std::thread::sleep(std::time::Duration::from_secs(3));
    });

    let event_bus = AppEventBus::default();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());
    let (command_tx, command_rx) = mpsc::channel(32);
    let command_bus = AppCommandBus::new(command_tx);
    let mut dispatcher = RuntimeDispatcher::new(command_rx, event_bus.clone());
    dispatcher.set_lv1(Some(lv1));
    let fade = spawn_engine(command_bus.clone(), event_bus);
    dispatcher.set_fade(Some(fade.clone()));
    tokio::spawn(async move { dispatcher.run().await });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        command_bus.start_fade(FadeConfig {
            targets: vec![FadeTarget {
                group: 0,
                channel: 0,
                target_db: -10.0,
            }],
            duration_ms: 500,
            curve: FadeCurve::Linear,
        }),
    )
    .await
    .expect("start_fade timed out")
    .unwrap();

    assert_eq!(result, ());

    drop(fade);
}

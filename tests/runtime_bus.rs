use lv1_scene_fade_utility::fade::curve::FadeCurve;
use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::fade::types::{FadeConfig, FadeSceneIdentity, FadeTarget};
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::SceneState;
use lv1_scene_fade_utility::lv1::state::spawn_actor;
use lv1_scene_fade_utility::lv1::tcp::encode_frame;
use lv1_scene_fade_utility::osc::OscArg;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus};
use std::io::Write;
use std::net::TcpListener;

#[tokio::test]
async fn app_event_bus_carries_lv1_events_without_actor_subscriber_api() {
    let bus = AppEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(SceneState {
        index: 4,
        name: "Outro".to_string(),
    })));

    let event = rx.recv().await.unwrap();
    match event {
        AppEvent::Lv1(Lv1Event::SceneChanged(scene)) => {
            assert_eq!(scene.index, 4);
            assert_eq!(scene.name, "Outro");
        }
        other => panic!("unexpected event: {other:?}"),
    }
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
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_lv1(Some(lv1)).await;
    let fade = spawn_engine(command_bus.clone(), event_bus);
    command_bus.set_fade(Some(fade.clone())).await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        command_bus.start_fade(FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
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

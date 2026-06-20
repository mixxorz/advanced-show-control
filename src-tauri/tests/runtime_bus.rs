use advanced_show_control::fade::{
    FadeCommand, FadeConfig, FadeCurve, FadeParameter, FadeSceneIdentity, FadeTarget, spawn_engine,
};
use advanced_show_control::lv1::{Lv1Event, SceneState, encode_frame, spawn_actor};
use advanced_show_control::osc::OscArg;
use advanced_show_control::runtime::commands::AppCommandBus;
use advanced_show_control::runtime::events::{AppEvent, AppEventBus};
use std::io::Write;
use std::net::TcpListener;
use std::time::Duration;

#[tokio::test]
async fn app_event_bus_carries_lv1_events_without_actor_subscriber_api() {
    let bus = AppEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.publish(AppEvent::Lv1 {
        generation: 0,
        event: Lv1Event::SceneChanged(SceneState {
            index: 4,
            name: "Outro".to_string(),
        }),
    });

    let event = rx.recv().await.unwrap();
    match event {
        AppEvent::Lv1 {
            event: Lv1Event::SceneChanged(scene),
            ..
        } => {
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
    let mut events = event_bus.subscribe();
    let lv1 = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);
    let command_bus = AppCommandBus::new();
    let fade = spawn_engine(command_bus.clone(), lv1, event_bus, 0);

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1 {
                    event: Lv1Event::ChannelTopologyChanged(_),
                    ..
                }) => break,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    panic!("event stream closed before /Channels was processed")
                }
            }
        }
    })
    .await
    .expect("actor did not process /Channels within timeout");

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let (reply, rx) = tokio::sync::oneshot::channel();
        fade.send(FadeCommand::RecallSceneFade {
            config: FadeConfig {
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
            expected_generation: None,
            reply: Some(reply),
        })
        .await
        .unwrap();
        rx.await.unwrap()
    })
    .await
    .expect("start_fade timed out")
    .unwrap();

    drop(fade);
}

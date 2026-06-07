use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::runtime::commands::AppCommandBus;
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use tokio::task::JoinHandle;

use crate::app_state::{SceneRecallDecision, ShellState};

pub fn spawn_scene_recall_fader(
    state: ShellState,
    generation: u64,
    command_bus: AppCommandBus,
    event_bus: AppEventBus,
) -> JoinHandle<()> {
    let mut events = event_bus.subscribe();

    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
                    match state
                        .prepare_scene_recall_fade_for_generation(generation, &scene)
                        .await
                    {
                        SceneRecallDecision::Start(request) => {
                            if command_bus.abort_all_fades().await.is_ok() {
                                state
                                    .log_scene_recall_fader_info(format!(
                                        "Previous fade aborted before auto fade for scene {}",
                                        request.scene_label
                                    ))
                                    .await;
                            }

                            if command_bus.start_fade(request.fade_config).await.is_ok() {
                                state
                                    .log_scene_recall_fader_info(format!(
                                        "Auto fade started for scene {}",
                                        request.scene_label
                                    ))
                                    .await;
                            }
                        }
                        SceneRecallDecision::Skip
                        | SceneRecallDecision::Blocked
                        | SceneRecallDecision::StaleGeneration => {}
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("scene-recall-fader", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

use tokio::sync::oneshot;

use super::{
    CueListsCommand, CueListsCommandResult, CueListsEvent, CueListsProjectionReason,
    CueListsProjectionState, CueListsState,
};
use crate::runtime::events::{AppEvent, AppEventBus};

pub(crate) struct CueLists {
    pub state: CueListsState,
    event_bus: AppEventBus,
    pending_recall: Option<PendingRecall>,
}

struct PendingRecall {
    entry_id: uuid::Uuid,
    reply: CueRecallReply,
    response: oneshot::Receiver<
        Result<crate::scenes::RecallSceneResult, crate::runtime::errors::AppCommandError>,
    >,
}

impl CueLists {
    pub fn new(event_bus: AppEventBus) -> Self {
        Self {
            state: CueListsState::default(),
            event_bus,
            pending_recall: None,
        }
    }

    pub fn recall_pending(&self) -> bool {
        self.pending_recall.is_some()
    }

    pub fn begin_recall(&mut self, reply: CueRecallReply) -> Option<crate::scenes::ScenesCommand> {
        let entry = match self.state.cued_entry() {
            Ok(entry) => entry,
            Err(error) => {
                let _ = reply.send(Err(crate::runtime::errors::AppCommandError::CommandFailed(
                    error,
                )));
                return None;
            }
        };
        let (scene_reply, response) = oneshot::channel();
        self.pending_recall = Some(PendingRecall {
            entry_id: entry.id,
            reply,
            response,
        });
        Some(crate::scenes::ScenesCommand::RecallScene {
            internal_scene_id: entry.scene_internal_id,
            reply: scene_reply,
        })
    }

    pub async fn complete_recall(&mut self) {
        use crate::runtime::errors::AppCommandError;
        let Some(pending) = &mut self.pending_recall else {
            return std::future::pending().await;
        };
        let result = (&mut pending.response)
            .await
            .unwrap_or(Err(AppCommandError::ReplyChannelClosed));
        let pending = self.pending_recall.take().unwrap();
        let result = result.and_then(|_| {
            if self.state.document().cued_cue_entry_id != Some(pending.entry_id) {
                return Err(AppCommandError::RecallCanceled("cued entry changed".into()));
            }
            let entry = self
                .state
                .advance_after_successful_recall()
                .map_err(AppCommandError::CommandFailed)?;
            self.publish(CueListsProjectionReason::CueListState, true);
            Ok(super::CueRecallResult {
                recalled_entry_id: entry.id,
                next_cued_entry_id: self.state.document().cued_cue_entry_id,
            })
        });
        let _ = pending.reply.send(result);
    }

    pub fn publish(&self, reason: CueListsProjectionReason, persisted_cue_list_edit: bool) {
        self.event_bus
            .publish(AppEvent::CueLists(CueListsEvent::StateChanged {
                reason,
                state: CueListsProjectionState {
                    document: self.state.document(),
                    last_recall_status: None,
                },
                persisted_cue_list_edit,
            }));
    }

    pub fn reconcile(&mut self, scenes: &[crate::scenes::SceneConfig]) {
        let result = self
            .state
            .reconcile(scenes.iter().map(|scene| scene.internal_scene_id));
        if let Some(cleared) = &result.cued_entry_cleared {
            tracing::warn!(
                event = "cue_cleared_missing_scene",
                cue_list_id = %cleared.cue_list_id,
                cue_entry_id = %cleared.cue_entry_id,
                scene_internal_id = %cleared.scene_internal_id,
                "Cued entry cleared because its scene is unavailable."
            );
        }
        if result.active_cue_list_cleared || result.cued_entry_cleared.is_some() {
            self.publish(CueListsProjectionReason::CueListState, true);
        }
    }

    pub fn dispatch(&mut self, command: CueListsCommand) {
        let state = &mut self.state;
        let changed = |changed| CueListsCommandResult {
            changed,
            cue_list: None,
            entry: None,
        };
        let (reply, result) = match command {
            CueListsCommand::InitialProjectionState { reply } => {
                let _ = reply.send(CueListsProjectionState {
                    document: state.document(),
                    last_recall_status: None,
                });
                return;
            }
            CueListsCommand::GetCueListDocument { reply } => {
                let _ = reply.send(state.document());
                return;
            }
            CueListsCommand::ReplaceCueListDocument {
                document,
                valid_scene_ids,
                persisted_cue_list_edit,
                reply,
            } => {
                let reconciliation = state.replace_document(document, valid_scene_ids);
                self.publish(
                    CueListsProjectionReason::FileReplacement,
                    persisted_cue_list_edit
                        || reconciliation.active_cue_list_cleared
                        || reconciliation.cued_entry_cleared.is_some(),
                );
                if let Some(reply) = reply {
                    let _ = reply.send(changed(true));
                }
                return;
            }
            CueListsCommand::CreateCueList { name, reply } => (
                reply,
                state
                    .create_cue_list(name)
                    .map(|cue_list| CueListsCommandResult {
                        cue_list: Some(cue_list),
                        ..changed(true)
                    }),
            ),
            CueListsCommand::RenameCueList {
                cue_list_id,
                name,
                reply,
            } => (
                reply,
                state
                    .rename_cue_list(cue_list_id, name)
                    .map(|()| changed(true)),
            ),
            CueListsCommand::DeleteCueList { cue_list_id, reply } => (
                reply,
                state.delete_cue_list(cue_list_id).map(|()| changed(true)),
            ),
            CueListsCommand::ReorderCueLists { ordered_ids, reply } => (
                reply,
                state.reorder_cue_lists(ordered_ids).map(|()| changed(true)),
            ),
            CueListsCommand::SetActiveCueList { cue_list_id, reply } => {
                (reply, state.set_active_cue_list(cue_list_id).map(changed))
            }
            CueListsCommand::AddSceneToActiveCueList {
                scene_internal_id,
                insert_index,
                reply,
            } => (
                reply,
                state
                    .add_scene_to_active_cue_list(scene_internal_id, insert_index)
                    .map(|entry| CueListsCommandResult {
                        entry: Some(entry),
                        ..changed(true)
                    }),
            ),
            CueListsCommand::RemoveCueEntry {
                cue_entry_id,
                reply,
            } => (
                reply,
                state.remove_cue_entry(cue_entry_id).map(|()| changed(true)),
            ),
            CueListsCommand::ReorderCueEntries {
                ordered_entry_ids,
                reply,
            } => (
                reply,
                state
                    .reorder_cue_entries(ordered_entry_ids)
                    .map(|()| changed(true)),
            ),
            CueListsCommand::CueEntry {
                cue_entry_id,
                reply,
            } => (reply, state.cue_entry(cue_entry_id).map(|()| changed(true))),
            CueListsCommand::RecallCuedCue { .. } | CueListsCommand::Shutdown => {
                unreachable!("handled by the session command loop")
            }
        };
        if result.as_ref().is_ok_and(|result| result.changed) {
            self.publish(CueListsProjectionReason::CueListState, true);
        }
        if let Some(reply) = reply {
            let _ = reply.send(result);
        }
    }
}

pub(crate) type CueRecallReply =
    oneshot::Sender<Result<super::CueRecallResult, crate::runtime::errors::AppCommandError>>;

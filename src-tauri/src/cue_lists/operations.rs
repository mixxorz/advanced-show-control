use tokio::sync::oneshot;

use super::{CueListsCommand, CueListsCommandResult, CueListsProjectionState, CueListsState};
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

    /// @cc [owner:mixxorz,label:safety;product] recall-start-requires-current-cue
    /// The owning command loop MUST call this method only when `recall_pending()` is false. Recall
    /// start MUST resolve the cued entry from the active list before creating a scene-recall command.
    /// Missing or inconsistent cue state MUST return `CommandFailed` to the caller and MUST NOT create
    /// pending recall work.
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

    /// @cc [owner:mixxorz,label:safety] replacement-cancels-pending-cue-recall
    /// Canceling a pending cue recall for session replacement MUST remove the pending operation and
    /// return `RecallCanceled` to its caller without advancing or otherwise editing the cue document.
    pub fn cancel_recall(&mut self) {
        if let Some(pending) = self.pending_recall.take() {
            let _ = pending.reply.send(Err(
                crate::runtime::errors::AppCommandError::RecallCanceled(
                    "session was replaced".into(),
                ),
            ));
        }
    }

    /// @cc [owner:mixxorz,label:safety;product] advance-only-after-dispatch-success
    /// Completion MUST advance to the next entry only after the scene recall reports successful LV1
    /// dispatch and the same entry remains cued. Dispatch failure, reply-channel failure, or changed cue
    /// identity MUST return an error and MUST NOT advance or publish a cue-list edit.
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
            self.publish();
            Ok(super::CueRecallResult {
                recalled_entry_id: entry.id,
                next_cued_entry_id: self.state.document().cued_cue_entry_id,
            })
        });
        let _ = pending.reply.send(result);
    }

    fn publish(&self) {
        self.event_bus
            .publish(AppEvent::CueLists(CueListsProjectionState {
                document: self.state.document(),
                last_recall_status: None,
            }));
    }

    /// @cc [owner:mixxorz,label:persistence;safety] reconciliation-publishes-selection-clears
    /// Scene reconciliation MUST retain entries whose scene UUID is absent and publish the reconciled
    /// document whenever active or cued selection is cleared. A cue cleared specifically because its
    /// scene is absent MUST also emit the user-visible `cue_cleared_missing_scene` warning.
    pub fn reconcile(&mut self, scenes: &[crate::scenes::SceneConfig]) {
        let result = self
            .state
            .reconcile(scenes.iter().map(|scene| scene.internal_scene_id));
        if let Some(cleared) = &result.cued_entry_cleared_for_missing_scene {
            tracing::warn!(
                event = "cue_cleared_missing_scene",
                cue_list_id = %cleared.cue_list_id,
                cue_entry_id = %cleared.cue_entry_id,
                scene_internal_id = %cleared.scene_internal_id,
                "Cued entry cleared because its scene is unavailable."
            );
        }
        if result.active_cue_list_cleared || result.cued_entry_cleared {
            self.publish();
        }
    }

    /// @cc [owner:mixxorz,label:persistence] successful-commands-publish-persisted-edit
    /// Every successful cue-list mutation reported with `changed = true` MUST publish the resulting
    /// full document as an `AppEvent::CueLists` persisted edit before replying. Rejected commands MUST
    /// return their domain error without publishing an edit.
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
            self.publish();
        }
        if let Some(reply) = reply {
            let _ = reply.send(result);
        }
    }
}

pub(crate) type CueRecallReply =
    oneshot::Sender<Result<super::CueRecallResult, crate::runtime::errors::AppCommandError>>;

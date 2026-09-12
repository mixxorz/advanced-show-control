use thiserror::Error;

/// @cc [owner:mixxorz,label:product] frontend-safe-command-errors
/// Static variants MUST render as complete user-facing messages. `CommandFailed` display MUST add
/// its generic command context and `RecallCanceled` display MUST retain its cancellation context;
/// presentation mapping MAY deliberately return only the contained `CommandFailed` message while using
/// every other variant's complete display text.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AppCommandError {
    #[error("LV1 actor is unavailable")]
    Lv1Unavailable,
    #[error("fade engine is unavailable")]
    FadeUnavailable,
    #[error("show state is unavailable")]
    ShowUnavailable,
    #[error("scene state is unavailable")]
    ScenesUnavailable,
    #[error("app command reply channel is closed")]
    ReplyChannelClosed,
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("generation is stale")]
    StaleGeneration,
    #[error("scene recall queue is full")]
    RecallQueueFull,
    #[error("scene recall canceled: {0}")]
    RecallCanceled(String),
}

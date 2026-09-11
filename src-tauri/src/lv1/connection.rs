use tokio::sync::oneshot;

use crate::runtime::errors::AppCommandError;
use crate::runtime::generation::RuntimeGeneration;

use super::{Lv1ActorError, Lv1ActorHandle, Lv1Command};

/// Pins an LV1 endpoint to one runtime generation; it never switches actors.
/// Checks fence mailbox admission, not bytes already handed to the transport.
#[derive(Clone)]
pub(crate) struct Lv1Connection {
    handle: Lv1ActorHandle,
    authority: RuntimeGeneration,
    generation: u64,
}

impl Lv1Connection {
    pub(crate) fn new(
        handle: Lv1ActorHandle,
        authority: RuntimeGeneration,
        generation: u64,
    ) -> Self {
        Self {
            handle,
            authority,
            generation,
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) async fn ensure_current(&self) -> Result<(), AppCommandError> {
        self.if_current(|| ())
            .await
            .ok_or(AppCommandError::StaleGeneration)
    }

    pub(crate) async fn if_current<T>(&self, operation: impl FnOnce() -> T) -> Option<T> {
        self.authority.if_current(self.generation, operation).await
    }

    pub(crate) async fn send(&self, command: Lv1Command) -> Result<(), AppCommandError> {
        self.send_checked(command, || Ok(())).await
    }

    pub(crate) async fn send_checked(
        &self,
        command: Lv1Command,
        validate: impl FnOnce() -> Result<(), AppCommandError>,
    ) -> Result<(), AppCommandError> {
        self.ensure_current().await?;
        let permit = self.handle.reserve().await;
        // Revocation wins even when the wait ended because the mailbox closed.
        self.if_current(|| {
            let permit = permit.map_err(|error| match error {
                Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
                other => AppCommandError::CommandFailed(other.to_string()),
            })?;
            validate()?;
            permit.send(command);
            Ok(())
        })
        .await
        .ok_or(AppCommandError::StaleGeneration)?
    }

    pub(crate) async fn request<T>(
        &self,
        command: impl FnOnce(oneshot::Sender<T>) -> Lv1Command,
    ) -> Result<T, AppCommandError> {
        self.request_checked(command, || Ok(())).await
    }

    pub(crate) async fn request_checked<T>(
        &self,
        command: impl FnOnce(oneshot::Sender<T>) -> Lv1Command,
        validate: impl FnOnce() -> Result<(), AppCommandError>,
    ) -> Result<T, AppCommandError> {
        let (reply, response) = oneshot::channel();
        self.send_checked(command(reply), validate).await?;
        let result = response
            .await
            .map_err(|_| AppCommandError::ReplyChannelClosed);
        self.ensure_current().await?;
        result
    }
}

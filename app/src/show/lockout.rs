use tokio::sync::watch;

/// @cc [owner:mixxorz,label:safety] lockout-reader-is-latest-value
/// The lockout reader MUST expose Show's latest accepted lockout value without a reverse mailbox
/// dependency; clones MUST observe subsequent changes through the shared watch channel.
#[derive(Clone)]
pub struct ShowLockoutReader {
    receiver: watch::Receiver<bool>,
}

impl ShowLockoutReader {
    pub(super) fn new(receiver: watch::Receiver<bool>) -> Self {
        Self { receiver }
    }

    pub fn current(&self) -> bool {
        *self.receiver.borrow()
    }

    pub async fn changed(&mut self) -> Result<bool, watch::error::RecvError> {
        self.receiver.changed().await?;
        Ok(self.current())
    }
}

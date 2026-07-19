use tokio::sync::watch;

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

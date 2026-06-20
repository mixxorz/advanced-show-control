use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct RuntimeGeneration {
    current: Arc<Mutex<u64>>,
}

impl RuntimeGeneration {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn current(&self) -> u64 {
        *self.current.lock().await
    }

    pub(crate) async fn set(&self, generation: u64) {
        *self.current.lock().await = generation;
    }
}

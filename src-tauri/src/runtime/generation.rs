use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Clone, Debug, Default)]
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

    #[cfg(test)]
    pub(crate) async fn set(&self, generation: u64) {
        *self.current.lock().await = generation;
    }

    pub(crate) async fn advance(&self) -> u64 {
        let mut current = self.current.lock().await;
        *current = current.saturating_add(1);
        *current
    }

    pub(crate) async fn if_current<T>(
        &self,
        expected: u64,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        let current = self.current.lock().await;
        (*current == expected).then(operation)
    }
}

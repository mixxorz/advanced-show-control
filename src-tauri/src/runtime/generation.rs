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
        *current = current.checked_add(1).expect("runtime generation overflow");
        *current
    }

    pub(crate) async fn advance_if_current(&self, expected: u64) -> Option<u64> {
        let mut current = self.current.lock().await;
        if *current != expected {
            return None;
        }
        *current = current.checked_add(1).expect("runtime generation overflow");
        Some(*current)
    }

    pub(crate) async fn if_current<T>(
        &self,
        expected: u64,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        let current = self.current.lock().await;
        (*current == expected).then(operation)
    }

    #[cfg(test)]
    pub(crate) async fn hold_for_test(&self) -> impl Drop {
        self.current.clone().lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeGeneration;

    #[tokio::test]
    async fn advance_if_current_advances_only_the_matching_generation() {
        let generation = RuntimeGeneration::new();
        let first = generation.advance().await;

        assert_eq!(generation.advance_if_current(first).await, Some(first + 1));
        assert_eq!(generation.advance_if_current(first).await, None);
        assert_eq!(generation.current().await, first + 1);
    }

    #[tokio::test]
    #[should_panic(expected = "runtime generation overflow")]
    async fn advance_fails_explicitly_at_u64_max() {
        let generation = RuntimeGeneration::new();
        generation.set(u64::MAX).await;

        generation.advance().await;
    }

    #[tokio::test]
    #[should_panic(expected = "runtime generation overflow")]
    async fn advance_if_current_fails_explicitly_at_u64_max() {
        let generation = RuntimeGeneration::new();
        generation.set(u64::MAX).await;

        generation.advance_if_current(u64::MAX).await;
    }
}

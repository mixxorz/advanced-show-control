use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Clone, Debug, Default)]
pub struct RuntimeGeneration {
    current: Arc<Mutex<u64>>,
    transition: Arc<Mutex<()>>,
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
        let _transition = self.transition.lock().await;
        let mut current = self.current.lock().await;
        *current = current.checked_add(1).expect("runtime generation overflow");
        *current
    }

    pub(crate) async fn advance_if_current(&self, expected: u64) -> Option<u64> {
        let _transition = self.transition.lock().await;
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

    pub(crate) async fn if_current_async<T, F, Fut>(&self, expected: u64, operation: F) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let _transition = self.transition.lock().await;
        let current = self.current.lock().await;
        if *current != expected {
            return None;
        }
        drop(current);
        Some(operation().await)
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
    async fn async_current_guard_serializes_generation_advance_until_operation_finishes() {
        let generation = RuntimeGeneration::new();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let guarded_generation = generation.clone();
        let operation = tokio::spawn(async move {
            guarded_generation
                .if_current_async(0, || async move {
                    entered_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    42
                })
                .await
        });

        entered_rx.await.unwrap();
        assert_eq!(generation.current().await, 0);
        assert_eq!(generation.if_current(0, || 7).await, Some(7));

        let mut advance = tokio::spawn({
            let generation = generation.clone();
            async move { generation.advance().await }
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut advance,)
                .await
                .is_err()
        );

        release_tx.send(()).unwrap();
        assert_eq!(operation.await.unwrap(), Some(42));
        assert_eq!(advance.await.unwrap(), 1);
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

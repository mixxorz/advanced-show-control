use crate::runtime::events::AppEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SmokeStepResult {
    pub ok: bool,
    pub step: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SmokeBackendResult {
    pub ok: bool,
    pub test_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub steps: Vec<SmokeStepResult>,
    pub observed_events: Vec<String>,
    pub observed_traces: Vec<crate::smoke::SmokeTraceEvent>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SmokeTestChannel {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SmokeTestParams {
    pub scene_a_id: String,
    pub scene_b_id: String,
    pub channel: SmokeTestChannel,
    pub tolerance_db: f64,
    pub minimum_movement_db: f64,
    pub timeout_ms: u64,
    pub sample_interval_ms: u64,
}

pub fn pass_step(step: impl Into<String>, message: impl Into<String>) -> SmokeStepResult {
    SmokeStepResult {
        ok: true,
        step: step.into(),
        message: message.into(),
        observed: None,
    }
}

pub fn fail_step(
    step: impl Into<String>,
    message: impl Into<String>,
    observed: Value,
) -> SmokeStepResult {
    SmokeStepResult {
        ok: false,
        step: step.into(),
        message: message.into(),
        observed: Some(observed),
    }
}

pub async fn wait_for_event(
    rx: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    timeout: std::time::Duration,
    mut predicate: impl FnMut(&AppEvent) -> bool,
) -> Result<AppEvent, String> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "timed out after {} ms waiting for app event",
                timeout.as_millis()
            ));
        }

        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(event)) if predicate(&event) => return Ok(event),
            Ok(Ok(_event)) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_count))) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                return Err("app event bus closed while waiting for event".to_string());
            }
            Err(_) => {
                return Err(format!(
                    "timed out after {} ms waiting for app event",
                    timeout.as_millis()
                ));
            }
        }
    }
}

pub fn summarize_app_event(event: &AppEvent) -> String {
    format!("{event:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_backend_result_serializes_camel_case() {
        let result = SmokeBackendResult {
            ok: false,
            test_id: "connection".to_string(),
            started_at: "2026-06-21T00:00:00Z".to_string(),
            finished_at: "2026-06-21T00:00:01Z".to_string(),
            steps: vec![fail_step(
                "connect",
                "not connected",
                serde_json::json!({"connection":"disconnected"}),
            )],
            observed_events: vec!["Runtime".to_string()],
            observed_traces: vec![],
        };

        let json = serde_json::to_value(result).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["testId"], "connection");
        assert_eq!(json["steps"][0]["step"], "connect");
        assert_eq!(json["observedEvents"][0], "Runtime");
    }

    #[tokio::test]
    async fn wait_for_event_returns_matching_event() {
        let bus = crate::runtime::events::AppEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish_runtime_generation_changed(7);

        let found = wait_for_event(&mut rx, std::time::Duration::from_millis(50), |event| {
            matches!(event, crate::runtime::events::AppEvent::Runtime(_))
        })
        .await
        .unwrap();

        assert!(matches!(
            found,
            crate::runtime::events::AppEvent::Runtime(_)
        ));
    }

    #[tokio::test]
    async fn wait_for_event_times_out_without_match() {
        let bus = crate::runtime::events::AppEventBus::new(16);
        let mut rx = bus.subscribe();

        let err = wait_for_event(&mut rx, std::time::Duration::from_millis(10), |_| false)
            .await
            .unwrap_err();

        assert!(err.contains("timed out"));
    }
}

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

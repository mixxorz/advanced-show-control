#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneRecallEvent {
    Skipped { scene_label: String, reason: String },
    Blocked { scene_label: String, reason: String },
    Ready { scene_label: String, target_count: usize },
    StartRequested { scene_label: String },
}

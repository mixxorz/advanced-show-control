#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowEvent {
    StateChanged,
    SceneConfigChanged { scene_id: String },
    LockoutChanged { enabled: bool },
}

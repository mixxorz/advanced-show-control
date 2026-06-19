#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowEvent {
    SnapshotChanged { reason: ShowSnapshotChange },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowSnapshotChange {
    CueScene,
    Lockout,
    SceneDuration,
    SceneScopeFaders,
    SceneScopePan,
    ChannelScope,
    AllChannelsScope,
    StoreSceneConfig,
    SceneListReconciled,
    SnapshotReplaced,
    Cleared,
}

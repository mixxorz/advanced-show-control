use crate::fade::curve::FadeCurve;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FadeSceneIdentity {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FadeConfig {
    pub scene: FadeSceneIdentity,
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

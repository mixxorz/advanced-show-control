use crate::fade::curve::FadeCurve;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FadeParameter {
    FaderDb,
    Pan,
    Balance,
    Width,
}

impl FadeParameter {
    pub fn is_pan_family(self) -> bool {
        matches!(self, Self::Pan | Self::Balance | Self::Width)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FadeTargetKey {
    pub group: i32,
    pub channel: i32,
    pub parameter: FadeParameter,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FadeSceneIdentity {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub parameter: FadeParameter,
    pub target: f64,
}

impl FadeTarget {
    pub fn key(&self) -> FadeTargetKey {
        FadeTargetKey {
            group: self.group,
            channel: self.channel,
            parameter: self.parameter,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FadeConfig {
    pub scene: FadeSceneIdentity,
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

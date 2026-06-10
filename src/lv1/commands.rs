use tokio::sync::oneshot;

use super::events::Lv1ActorError;
use super::types::Lv1StateSnapshot;

pub enum Lv1Command {
    GetState {
        reply: oneshot::Sender<Lv1StateSnapshot>,
    },
    WriteBatch(Vec<Lv1ParameterWrite>),
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetPan {
        group: i32,
        channel: i32,
        value: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetBalance {
        group: i32,
        channel: i32,
        value: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetWidth {
        group: i32,
        channel: i32,
        value: f64,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    SetMute {
        group: i32,
        channel: i32,
        muted: bool,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
    Flush {
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lv1WriteParameter {
    FaderDb,
    Pan,
    Balance,
    Width,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lv1ParameterWrite {
    pub group: i32,
    pub channel: i32,
    pub parameter: Lv1WriteParameter,
    pub value: f64,
}

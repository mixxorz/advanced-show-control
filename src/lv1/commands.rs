use tokio::sync::oneshot;

use super::messages::Lv1ActorError;
use super::types::Lv1StateSnapshot;

pub enum Lv1Command {
    GetState {
        reply: oneshot::Sender<Lv1StateSnapshot>,
    },
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
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

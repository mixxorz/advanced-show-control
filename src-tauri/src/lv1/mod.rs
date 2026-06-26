mod actor;
mod commands;
mod discovery;
mod events;
mod handle;
pub mod osc;
mod parsers;
pub mod probe;
mod state;
mod tcp;
mod types;

pub use actor::{Lv1ActorTask, build_actor};
pub use commands::{Lv1Command, Lv1ParameterWrite, Lv1WriteParameter};
pub use discovery::{DiscoverOptions, DiscoveryEntry, discover, resolve_target};
pub use events::{Lv1ActorError, Lv1Event};
pub use handle::Lv1ActorHandle;
pub use probe::{TcpConnectProbeResult, probe_tcp_connect_latency};
pub use tcp::{Lv1Frame, Lv1TcpClient, decode_frame_payload, encode_frame, pong_for_ping};
pub use types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, PanMode, SceneListEntry, SceneState,
};

#[cfg(test)]
pub(crate) fn test_actor_handle(tx: tokio::sync::mpsc::Sender<Lv1Command>) -> Lv1ActorHandle {
    handle::Lv1ActorHandle::new(tx)
}

use advanced_show_control::lv1::discovery::DiscoveryEntry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveredLv1Status {
    Available,
    Connecting,
    Connected,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lv1SystemIdentity {
    pub uuid: Option<String>,
    pub host: Option<String>,
    pub address: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredLv1System {
    pub identity: Lv1SystemIdentity,
    pub latency_ms: Option<u64>,
    pub status: DiscoveredLv1Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectState {
    pub active: bool,
    pub attempt: u64,
}

pub fn identity_from_discovery(entry: &DiscoveryEntry) -> Option<Lv1SystemIdentity> {
    let address = entry.addresses.first()?.clone();
    let port = entry.port?;
    Some(Lv1SystemIdentity {
        uuid: entry.uuid.clone(),
        host: entry.host.clone(),
        address,
        port,
    })
}

use crate::lv1::DiscoveryEntry;
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

pub fn system_from_discovery(entry: &DiscoveryEntry) -> Option<DiscoveredLv1System> {
    Some(DiscoveredLv1System {
        identity: identity_from_discovery(entry)?,
        status: DiscoveredLv1Status::Available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_from_discovery_maps_identity_and_status() {
        let entry = DiscoveryEntry {
            service: "_waveslv113._tcp".to_string(),
            uuid: Some("lv1-demo".to_string()),
            host: Some("FOH LV1".to_string()),
            port: Some(22000),
            addresses: vec!["192.168.1.42".to_string()],
            ipv6: Vec::new(),
            source: "192.168.1.42".to_string(),
        };

        let system = system_from_discovery(&entry).expect("entry should map to modal system");

        assert_eq!(system.identity.address, "192.168.1.42");
        assert_eq!(system.identity.port, 22000);
        assert_eq!(system.status, DiscoveredLv1Status::Available);
    }
}

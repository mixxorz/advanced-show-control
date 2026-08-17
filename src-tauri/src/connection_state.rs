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

pub fn startup_auto_connect_target(
    remembered: &Lv1SystemIdentity,
    systems: &[DiscoveredLv1System],
) -> Option<Lv1SystemIdentity> {
    let available: Vec<_> = systems
        .iter()
        .filter(|system| system.status == DiscoveredLv1Status::Available)
        .collect();

    if let Some(uuid) = remembered.uuid.as_deref()
        && let Some(system) = available
            .iter()
            .find(|system| system.identity.uuid.as_deref() == Some(uuid))
    {
        return Some(system.identity.clone());
    }

    let host = remembered.host.as_deref()?.trim();
    if host.is_empty() {
        return None;
    }
    let mut matches = available
        .into_iter()
        .filter(|system| system.identity.host.as_deref().map(str::trim) == Some(host));
    let target = matches.next()?.identity.clone();
    matches.next().is_none().then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(uuid: Option<&str>, host: Option<&str>, address: &str) -> Lv1SystemIdentity {
        Lv1SystemIdentity {
            uuid: uuid.map(str::to_string),
            host: host.map(str::to_string),
            address: address.to_string(),
            port: 50000,
        }
    }

    fn system(
        uuid: Option<&str>,
        host: Option<&str>,
        address: &str,
        status: DiscoveredLv1Status,
    ) -> DiscoveredLv1System {
        DiscoveredLv1System {
            identity: identity(uuid, host, address),
            status,
        }
    }

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

    #[test]
    fn startup_target_prefers_uuid_over_hostname() {
        let remembered = identity(Some("uuid-1"), Some("LV1-FOH"), "192.168.1.35");
        let systems = vec![
            system(
                Some("uuid-2"),
                Some("LV1-FOH"),
                "10.0.0.20",
                DiscoveredLv1Status::Available,
            ),
            system(
                Some("uuid-1"),
                Some("Renamed"),
                "10.0.0.21",
                DiscoveredLv1Status::Available,
            ),
        ];

        assert_eq!(
            startup_auto_connect_target(&remembered, &systems)
                .unwrap()
                .address,
            "10.0.0.21"
        );
    }

    #[test]
    fn startup_target_uses_one_exact_trimmed_hostname() {
        let remembered = identity(None, Some(" LV1-FOH "), "192.168.1.35");
        let systems = vec![system(
            None,
            Some("LV1-FOH"),
            "10.0.0.20",
            DiscoveredLv1Status::Available,
        )];

        assert_eq!(
            startup_auto_connect_target(&remembered, &systems)
                .unwrap()
                .address,
            "10.0.0.20"
        );
    }

    #[test]
    fn startup_target_rejects_ambiguous_or_address_only_matches() {
        let remembered = identity(None, Some("LV1-FOH"), "10.0.0.20");
        let duplicate_hosts = vec![
            system(
                None,
                Some("LV1-FOH"),
                "10.0.0.20",
                DiscoveredLv1Status::Available,
            ),
            system(
                None,
                Some("LV1-FOH"),
                "10.0.0.21",
                DiscoveredLv1Status::Available,
            ),
        ];
        assert!(startup_auto_connect_target(&remembered, &duplicate_hosts).is_none());

        let address_only = vec![system(
            None,
            Some("Different"),
            "10.0.0.20",
            DiscoveredLv1Status::Available,
        )];
        assert!(startup_auto_connect_target(&remembered, &address_only).is_none());
    }

    #[test]
    fn startup_target_ignores_unavailable_uuid_and_hostname_matches() {
        let uuid_remembered = identity(Some("uuid-1"), Some("LV1-FOH"), "192.168.1.35");
        let hostname_remembered = identity(None, Some("LV1-FOH"), "192.168.1.35");
        let systems = vec![system(
            Some("uuid-1"),
            Some("LV1-FOH"),
            "10.0.0.20",
            DiscoveredLv1Status::Unavailable,
        )];

        assert!(startup_auto_connect_target(&uuid_remembered, &systems).is_none());
        assert!(startup_auto_connect_target(&hostname_remembered, &systems).is_none());
    }

    #[test]
    fn startup_target_rejects_blank_hostname_without_using_address_or_port() {
        let remembered = Lv1SystemIdentity {
            uuid: None,
            host: Some("   ".to_string()),
            address: "10.0.0.20".to_string(),
            port: 50_000,
        };
        let systems = vec![DiscoveredLv1System {
            identity: Lv1SystemIdentity {
                uuid: None,
                host: Some("Different LV1".to_string()),
                address: "10.0.0.20".to_string(),
                port: 50_000,
            },
            status: DiscoveredLv1Status::Available,
        }];

        assert_eq!(startup_auto_connect_target(&remembered, &systems), None);
    }
}

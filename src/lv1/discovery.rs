//! Waves LV1 custom /zDNS discovery.

use crate::osc::{decode_packet, OscArg, OscError};

pub const MCAST_ADDR: &str = "225.1.1.1";
pub const MCAST_PORT: u16 = 13337;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DiscoveryEntry {
    pub service: String,
    pub uuid: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub addresses: Vec<String>,
    pub ipv6: Vec<String>,
    pub source: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("OSC error: {0}")]
    Osc(#[from] OscError),
    #[error("not a zDNS packet")]
    NotZdns,
}

fn ipv4_like(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok())
}

fn ipv6_like(value: &str) -> bool {
    value.contains(':')
}

pub fn rank_ip(ip: &str) -> i32 {
    if ip.starts_with("127.") {
        -100
    } else if ip.starts_with("169.254.") {
        -50
    } else if ip.starts_with("192.168.56.") {
        20
    } else if ip.starts_with("172.") {
        let second = ip
            .split('.')
            .nth(1)
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(0);
        if (16..=31).contains(&second) {
            30
        } else {
            40
        }
    } else if ip.starts_with("192.168.") {
        100
    } else if ip.starts_with("10.") {
        90
    } else {
        40
    }
}

pub fn parse_zdns_packet(bytes: &[u8], source: &str) -> Result<DiscoveryEntry, DiscoveryError> {
    let msg = decode_packet(bytes)?;
    if msg.address != "/zDNS" {
        return Err(DiscoveryError::NotZdns);
    }

    let Some(OscArg::String(service)) = msg.args.first() else {
        return Err(DiscoveryError::NotZdns);
    };

    let uuid = match msg.args.get(1) {
        Some(OscArg::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    };

    let mut host = None;
    let mut port = None;
    let mut addresses = Vec::new();
    let mut ipv6 = Vec::new();

    for arg in msg.args.iter().skip(2) {
        match arg {
            OscArg::String(value) if ipv4_like(value) => addresses.push(value.clone()),
            OscArg::String(value) if ipv6_like(value) => ipv6.push(value.clone()),
            OscArg::String(value) if host.is_none() && !value.is_empty() => {
                host = Some(value.clone());
            }
            OscArg::Int(value) if port.is_none() && *value > 1024 && *value < 65536 => {
                port = Some(*value as u16);
            }
            _ => {}
        }
    }

    addresses.sort_by_key(|ip| std::cmp::Reverse(rank_ip(ip)));

    Ok(DiscoveryEntry {
        service: service.clone(),
        uuid,
        host,
        port,
        addresses,
        ipv6,
        source: source.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc::encode_message;

    #[test]
    fn parses_zdns_packet_and_ranks_ipv4_addresses() {
        let packet = encode_message(
            "/zDNS",
            &[
                OscArg::String("_waveslv113._tcp".to_string()),
                OscArg::String("uuid-1".to_string()),
                OscArg::String("lv1-host".to_string()),
                OscArg::Int(50000),
                OscArg::String("172.20.1.9".to_string()),
                OscArg::String("192.168.1.10".to_string()),
                OscArg::String("fe80::1".to_string()),
            ],
        )
        .unwrap();

        let entry = parse_zdns_packet(&packet, "192.168.1.10").unwrap();

        assert_eq!(entry.service, "_waveslv113._tcp");
        assert_eq!(entry.uuid.as_deref(), Some("uuid-1"));
        assert_eq!(entry.host.as_deref(), Some("lv1-host"));
        assert_eq!(entry.port, Some(50000));
        assert_eq!(entry.addresses, vec!["192.168.1.10", "172.20.1.9"]);
        assert_eq!(entry.ipv6, vec!["fe80::1"]);
        assert_eq!(entry.source, "192.168.1.10");
    }

    #[test]
    fn rejects_non_zdns_packets() {
        let packet = encode_message("/not-zdns", &[]).unwrap();
        assert!(matches!(
            parse_zdns_packet(&packet, "127.0.0.1"),
            Err(DiscoveryError::NotZdns)
        ));
    }

    #[test]
    fn ranks_likely_lan_addresses_highest() {
        assert!(rank_ip("192.168.1.10") > rank_ip("172.20.1.9"));
        assert!(rank_ip("10.0.0.4") > rank_ip("169.254.1.1"));
        assert!(rank_ip("127.0.0.1") < rank_ip("169.254.1.1"));
    }
}

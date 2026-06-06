//! Waves LV1 custom /zDNS discovery.

use crate::osc::{decode_packet, OscArg, OscError};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

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

#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    pub timeout: std::time::Duration,
    pub filter_host_ip: Option<String>,
    pub filter_service: String,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            timeout: std::time::Duration::from_millis(6000),
            filter_host_ip: None,
            filter_service: "_waveslv113._tcp".to_string(),
        }
    }
}

pub fn entry_matches(entry: &DiscoveryEntry, service: &str, host_ip: Option<&str>) -> bool {
    if entry.service != service {
        return false;
    }

    match host_ip {
        Some(ip) => entry.addresses.iter().any(|address| address == ip),
        None => true,
    }
}

pub fn discover(options: DiscoverOptions) -> std::io::Result<Vec<DiscoveryEntry>> {
    use std::collections::BTreeMap;
    use std::net::{SocketAddrV4, UdpSocket};
    use std::time::Instant;

    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MCAST_PORT).into())?;
    let socket: UdpSocket = socket.into();
    socket.set_multicast_loop_v4(true)?;
    socket.join_multicast_v4(&MCAST_ADDR.parse::<Ipv4Addr>().unwrap(), &Ipv4Addr::UNSPECIFIED)?;

    let deadline = Instant::now() + options.timeout;
    let mut found = BTreeMap::<String, DiscoveryEntry>::new();
    let mut buf = [0_u8; 65_536];

    while Instant::now() < deadline {
        let Some(read_timeout) = read_timeout_for(deadline.saturating_duration_since(Instant::now()))
        else {
            break;
        };
        socket.set_read_timeout(Some(read_timeout))?;

        match socket.recv_from(&mut buf) {
            Ok((size, source)) => {
                if let Ok(entry) = parse_zdns_packet(&buf[..size], &source.ip().to_string()) {
                    if entry_matches(
                        &entry,
                        &options.filter_service,
                        options.filter_host_ip.as_deref(),
                    ) {
                        let key = dedupe_key(&entry);
                        found.entry(key).or_insert(entry);
                    }
                }
            }
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(err) => return Err(err),
        }
    }

    Ok(found.into_values().collect())
}

fn dedupe_key(entry: &DiscoveryEntry) -> String {
    if let Some(uuid) = &entry.uuid {
        return format!("uuid|{}|{}", entry.service, uuid);
    }

    let mut addresses = entry.addresses.clone();
    addresses.sort();
    let mut ipv6 = entry.ipv6.clone();
    ipv6.sort();

    format!(
        "fallback|{}|{:?}|{:?}|{}|{:?}|{:?}",
        entry.service, entry.host, entry.port, entry.source, addresses, ipv6
    )
}

fn read_timeout_for(remaining: std::time::Duration) -> Option<std::time::Duration> {
    if remaining.is_zero() {
        None
    } else {
        Some(remaining.min(std::time::Duration::from_millis(250)))
    }
}

fn ipv4_like(value: &str) -> bool {
    Ipv4Addr::from_str(value).is_ok()
}

fn ipv6_like(value: &str) -> bool {
    Ipv6Addr::from_str(value).is_ok()
}

pub fn rank_ip(ip: &str) -> i32 {
    let Ok(ip) = Ipv4Addr::from_str(ip) else {
        return 0;
    };
    let octets = ip.octets();

    if octets[0] == 127 {
        -100
    } else if octets[0] == 169 && octets[1] == 254 {
        -50
    } else if octets[0] == 192 && octets[1] == 168 && octets[2] == 56 {
        20
    } else if octets[0] == 192 && octets[1] == 168 {
        100
    } else if octets[0] == 10 {
        90
    } else if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        80
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

    #[test]
    fn ignores_invalid_address_like_strings() {
        let packet = encode_message(
            "/zDNS",
            &[
                OscArg::String("_waveslv113._tcp".to_string()),
                OscArg::String("uuid-1".to_string()),
                OscArg::String("lv1-host".to_string()),
                OscArg::Int(50000),
                OscArg::String("192.168.001.10".to_string()),
                OscArg::String("lv1-host:control".to_string()),
            ],
        )
        .unwrap();

        let entry = parse_zdns_packet(&packet, "192.168.1.10").unwrap();

        assert_eq!(entry.host.as_deref(), Some("lv1-host"));
        assert!(entry.addresses.is_empty());
        assert!(entry.ipv6.is_empty());
    }

    #[test]
    fn ranks_private_172_addresses_above_public_addresses() {
        assert!(rank_ip("172.16.0.1") > rank_ip("8.8.8.8"));
        assert!(rank_ip("172.31.255.254") > rank_ip("8.8.8.8"));
        assert!(rank_ip("172.15.255.255") <= rank_ip("8.8.8.8"));
        assert!(rank_ip("172.32.0.1") <= rank_ip("8.8.8.8"));
    }

    #[test]
    fn filter_entry_by_service_and_host_ip() {
        let entry = DiscoveryEntry {
            service: "_waveslv113._tcp".to_string(),
            uuid: Some("uuid-1".to_string()),
            host: Some("lv1-host".to_string()),
            port: Some(50000),
            addresses: vec!["192.168.1.10".to_string()],
            ipv6: vec![],
            source: "192.168.1.10".to_string(),
        };

        assert!(entry_matches(&entry, "_waveslv113._tcp", None));
        assert!(entry_matches(
            &entry,
            "_waveslv113._tcp",
            Some("192.168.1.10")
        ));
        assert!(!entry_matches(
            &entry,
            "_waveslv113._tcp",
            Some("10.0.0.4")
        ));
        assert!(!entry_matches(&entry, "_other._tcp", None));
    }

    #[test]
    fn discover_returns_empty_when_timeout_elapses_without_packets() {
        let entries = discover(DiscoverOptions {
            timeout: std::time::Duration::ZERO,
            ..DiscoverOptions::default()
        })
        .unwrap();

        assert!(entries.is_empty());
    }

    #[test]
    fn dedupe_key_prefers_uuid_when_present() {
        let mut entry = DiscoveryEntry {
            service: "_waveslv113._tcp".to_string(),
            uuid: Some("uuid-1".to_string()),
            host: Some("lv1-host".to_string()),
            port: Some(50000),
            addresses: vec!["192.168.1.10".to_string()],
            ipv6: vec![],
            source: "192.168.1.10".to_string(),
        };
        let key = dedupe_key(&entry);

        entry.host = Some("renamed-host".to_string());
        entry.port = Some(50001);
        entry.addresses = vec!["10.0.0.4".to_string()];
        entry.source = "10.0.0.4".to_string();

        assert_eq!(dedupe_key(&entry), key);
    }

    #[test]
    fn dedupe_key_without_uuid_keeps_distinct_sources_and_addresses() {
        let mut entry = DiscoveryEntry {
            service: "_waveslv113._tcp".to_string(),
            uuid: None,
            host: Some("lv1-host".to_string()),
            port: Some(50000),
            addresses: vec!["192.168.1.10".to_string()],
            ipv6: vec![],
            source: "192.168.1.10".to_string(),
        };
        let key = dedupe_key(&entry);

        entry.addresses = vec!["192.168.1.11".to_string()];
        entry.source = "192.168.1.11".to_string();

        assert_ne!(dedupe_key(&entry), key);
    }

    #[test]
    fn read_timeout_is_capped_to_remaining_deadline() {
        assert_eq!(
            read_timeout_for(std::time::Duration::from_millis(10)),
            Some(std::time::Duration::from_millis(10))
        );
        assert_eq!(
            read_timeout_for(std::time::Duration::from_secs(1)),
            Some(std::time::Duration::from_millis(250))
        );
        assert_eq!(read_timeout_for(std::time::Duration::ZERO), None);
    }
}

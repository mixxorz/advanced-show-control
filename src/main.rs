use clap::{Parser, Subcommand};
use lv1_scene_fade_utility::lv1::discovery::{DiscoverOptions, discover, resolve_target};
use lv1_scene_fade_utility::lv1::probe::{JsonlLogger, MessageKind, entry_for_message};
use lv1_scene_fade_utility::lv1::tcp::{Lv1TcpClient, decode_frame_payload, pong_for_ping};
use lv1_scene_fade_utility::osc::OscArg;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Parser)]
#[command(name = "lv1-probe")]
#[command(about = "Phase 1 Waves LV1 protocol discovery probe")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Scan the network for Waves LV1 devices via zDNS multicast")]
    Discover {
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long)]
        filter_host: Option<String>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Connect to an LV1 device and log all received OSC messages")]
    Listen {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long, default_value = "logs")]
        log_dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Send a gain command to an LV1 device output channel")]
    SetGain {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        group: i32,
        #[arg(long)]
        channel: i32,
        #[arg(long, allow_hyphen_values = true)]
        gain_db: f64,
    },
    #[command(about = "Connect to an LV1 device and print state changes to the terminal")]
    Monitor {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_cli_from(std::env::args_os()).unwrap_or_else(|err| err.exit());
    match cli.command {
        Command::Discover {
            timeout_ms,
            filter_host,
            json,
        } => run_discover(timeout_ms, filter_host, json),
        Command::Listen {
            host,
            port,
            timeout_ms,
            log_dir,
            json,
        } => run_listen(host, port, timeout_ms, log_dir, json),
        Command::SetGain {
            host,
            port,
            group,
            channel,
            gain_db,
        } => run_set_gain(host, port, group, channel, gain_db),
        Command::Monitor {
            host,
            port,
            timeout_ms,
        } => run_monitor(host, port, timeout_ms),
    }
}

fn parse_cli_from<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = std::iter::once(std::ffi::OsString::from("lv1-probe"))
        .chain(args.into_iter().skip(1).map(Into::into));
    Cli::try_parse_from(args)
}

fn run_discover(
    timeout_ms: u64,
    filter_host: Option<String>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = discover(DiscoverOptions {
        timeout: Duration::from_millis(timeout_ms),
        filter_host_ip: filter_host,
        ..DiscoverOptions::default()
    })?;

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        for entry in entries {
            println!(
                "service={} host={:?} port={:?} addresses={:?} source={}",
                entry.service, entry.host, entry.port, entry.addresses, entry.source
            );
        }
    }
    Ok(())
}

fn run_listen(
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
    log_dir: PathBuf,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("lv1-probe-{}.jsonl", unix_timestamp_secs()));
    let mut logger = JsonlLogger::create(&log_path)?;
    let (host, port) = resolve_target(host, port, timeout_ms)?;
    let mut client = Lv1TcpClient::connect(&host, port)?;
    client.register_myfoh("lv1-probe", &uuid::Uuid::new_v4().to_string())?;
    eprintln!("listening on {host}:{port}; writing {}", log_path.display());

    loop {
        for frame in client.read_available()? {
            let msg = decode_frame_payload(&frame)?;
            if let Some((address, args)) = pong_for_ping(&msg) {
                client.send(address, &args)?;
            }

            let entry = entry_for_message(
                "received",
                &msg,
                Some(frame.payload.len()),
                Some(frame.header),
            );
            if !json
                && matches!(
                    entry.kind,
                    MessageKind::Scene
                        | MessageKind::Fader
                        | MessageKind::Handshake
                        | MessageKind::Keepalive
                )
            {
                println!(
                    "{:?} {} {:?}",
                    entry.kind,
                    entry.address.as_deref().unwrap_or(""),
                    entry.args
                );
            }
            if json {
                println!("{}", serde_json::to_string(&entry)?);
            }
            logger.write(entry)?;
        }
    }
}

fn run_set_gain(
    host: Option<String>,
    port: Option<u16>,
    group: i32,
    channel: i32,
    gain_db: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let (host, port) = resolve_target(host, port, 6000)?;
    let mut client = Lv1TcpClient::connect(&host, port)?;
    client.register_myfoh("lv1-probe", &uuid::Uuid::new_v4().to_string())?;
    client.send(
        "/Set/Track/Out/Gain",
        &[
            OscArg::Int(group),
            OscArg::Int(channel),
            OscArg::Double(gain_db),
        ],
    )?;

    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        for frame in client.read_available()? {
            let msg = decode_frame_payload(&frame)?;
            if let Some((address, args)) = pong_for_ping(&msg) {
                client.send(address, &args)?;
            }
            println!("received {} {:?}", msg.address, msg.args);
        }
    }
    Ok(())
}

fn run_monitor(
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use lv1_scene_fade_utility::lv1::state::{Lv1Event, spawn_actor};

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let handle = spawn_actor(host.clone(), port);
        let mut events = handle.subscribe().await;

        while let Some(event) = events.recv().await {
            match event {
                Lv1Event::Connected => println!("[connected] {host}:{port}"),
                Lv1Event::Disconnected => println!("[disconnected] reconnecting in 3s..."),
                Lv1Event::SceneChanged(scene) => {
                    println!("[scene] index={} name={:?}", scene.index, scene.name);
                }
                Lv1Event::SceneListChanged(list) => {
                    println!("[scene-list] {} scenes", list.len());
                    for entry in &list {
                        println!("  [{}] {:?}", entry.index, entry.name);
                    }
                }
                Lv1Event::FaderChanged { group, channel, gain_db } => {
                    println!("[fader] group={group} ch={channel} {gain_db:.1} dB");
                }
                Lv1Event::ChannelTopologyChanged(channels) => {
                    println!("[channels] {} channels loaded", channels.len());
                }
            }
        }
    });

    Ok(())
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn parses_required_cli_commands_and_options() {
        let cli = Cli::try_parse_from([
            "lv1-probe",
            "discover",
            "--timeout-ms",
            "1000",
            "--filter-host",
            "192.168.1.10",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Command::Discover {
                timeout_ms,
                filter_host,
                json,
            } => {
                assert_eq!(timeout_ms, 1000);
                assert_eq!(filter_host.as_deref(), Some("192.168.1.10"));
                assert!(json);
            }
            other => panic!("expected discover command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "lv1-probe",
            "listen",
            "--host",
            "192.168.1.11",
            "--port",
            "50000",
            "--timeout-ms",
            "2000",
            "--log-dir",
            "logs-test",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Command::Listen {
                host,
                port,
                timeout_ms,
                log_dir,
                json,
            } => {
                assert_eq!(host.as_deref(), Some("192.168.1.11"));
                assert_eq!(port, Some(50000));
                assert_eq!(timeout_ms, 2000);
                assert_eq!(log_dir, std::path::PathBuf::from("logs-test"));
                assert!(json);
            }
            other => panic!("expected listen command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "lv1-probe",
            "set-gain",
            "--host",
            "192.168.1.12",
            "--port",
            "50001",
            "--group",
            "0",
            "--channel",
            "1",
            "--gain-db",
            "-12.5",
        ])
        .unwrap();

        match cli.command {
            Command::SetGain {
                host,
                port,
                group,
                channel,
                gain_db,
            } => {
                assert_eq!(host.as_deref(), Some("192.168.1.12"));
                assert_eq!(port, Some(50001));
                assert_eq!(group, 0);
                assert_eq!(channel, 1);
                assert_eq!(gain_db, -12.5);
            }
            other => panic!("expected set-gain command, got {other:?}"),
        }
    }

    #[test]
    fn help_uses_lv1_probe_name_even_when_binary_name_differs() {
        let err = parse_cli_from(["lv1-scene-fade-utility", "--help"]).unwrap_err();

        assert!(err.to_string().contains("Usage: lv1-probe <COMMAND>"));
    }

    #[test]
    fn parses_monitor_command() {
        let cli = Cli::try_parse_from([
            "lv1-probe",
            "monitor",
            "--host",
            "192.168.1.10",
            "--port",
            "50000",
            "--timeout-ms",
            "3000",
        ])
        .unwrap();

        match cli.command {
            Command::Monitor { host, port, timeout_ms } => {
                assert_eq!(host.as_deref(), Some("192.168.1.10"));
                assert_eq!(port, Some(50000));
                assert_eq!(timeout_ms, 3000);
            }
            other => panic!("expected monitor command, got {other:?}"),
        }
    }
}

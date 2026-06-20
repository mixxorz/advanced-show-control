use advanced_show_control::fade::{
    FadeConfig, FadeCurve, FadeEngineHandle, FadeEvent, FadeParameter, FadeSceneIdentity,
    FadeTarget, spawn_engine,
};
use advanced_show_control::lv1::probe::{JsonlLogger, MessageKind, entry_for_message};
use advanced_show_control::lv1::{
    ChannelInfo, DiscoverOptions, Lv1ActorHandle, Lv1Command, Lv1Event, Lv1TcpClient,
    decode_frame_payload, discover, pong_for_ping, resolve_target, spawn_actor,
};
use advanced_show_control::osc::OscArg;
use advanced_show_control::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CurveArg {
    Linear,
}

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
    #[command(about = "Experiment: listen, send pan/width/balance candidates, then quit")]
    PanProbe {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long, default_value = "logs/pan-write-test")]
        log_dir: PathBuf,
        #[arg(long, default_value_t = 0)]
        group: i32,
        #[arg(long, default_value_t = 4)]
        channel: i32,
        #[arg(long, allow_hyphen_values = true, default_value_t = 5.0)]
        pan: f64,
        #[arg(long, allow_hyphen_values = true, default_value_t = 0.5)]
        width: f64,
        #[arg(long, allow_hyphen_values = true, default_value_t = 5.0)]
        balance: f64,
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
    #[command(
        about = "Send repeated gain commands on a single connection and report echo rate and latency"
    )]
    RateTest {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 0)]
        group: i32,
        #[arg(long, default_value_t = 0)]
        channel: i32,
        #[arg(long, default_value_t = 25)]
        rate_hz: u64,
        #[arg(long, default_value_t = 40)]
        count: u64,
        #[arg(long, allow_hyphen_values = true, default_value_t = -20.0)]
        start_db: f64,
        #[arg(long, allow_hyphen_values = true, default_value_t = -10.0)]
        end_db: f64,
    },
    #[command(about = "Run a timed fade on a single LV1 channel")]
    FadeTest {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 0)]
        group: i32,
        #[arg(long, default_value_t = 0)]
        channel: i32,
        #[arg(long, allow_hyphen_values = true)]
        target_db: f64,
        #[arg(long, default_value_t = 4000)]
        duration_ms: u64,
        #[arg(long, value_enum, default_value_t = CurveArg::Linear)]
        curve: CurveArg,
    },
    #[command(about = "Run a whole-console LV1 fader sine-wave stress test")]
    Vegas {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
    },
    #[command(about = "Run a live LV1 pan/balance/width fade-engine smoke test")]
    PanFamilySmokeTest {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long, default_value_t = 6000)]
        timeout_ms: u64,
        #[arg(long, default_value = "logs/pan-family-smoke-test")]
        log_dir: PathBuf,
        #[arg(long, default_value_t = 0)]
        group: i32,
        #[arg(long, default_value_t = 2)]
        channel: i32,
    },
}

#[tokio::main]
async fn main() -> AppResult<()> {
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
        } => run_listen(host, port, timeout_ms, log_dir, json).await,
        Command::SetGain {
            host,
            port,
            group,
            channel,
            gain_db,
        } => run_set_gain(host, port, group, channel, gain_db).await,
        Command::PanProbe {
            host,
            port,
            timeout_ms,
            log_dir,
            group,
            channel,
            pan,
            width,
            balance,
        } => {
            run_pan_probe(PanProbeOptions {
                host,
                port,
                timeout_ms,
                log_dir,
                group,
                channel,
                pan,
                width,
                balance,
            })
            .await
        }
        Command::Monitor {
            host,
            port,
            timeout_ms,
        } => run_monitor(host, port, timeout_ms).await,
        Command::RateTest {
            host,
            port,
            group,
            channel,
            rate_hz,
            count,
            start_db,
            end_db,
        } => run_rate_test(host, port, group, channel, rate_hz, count, start_db, end_db).await,
        Command::FadeTest {
            host,
            port,
            timeout_ms,
            group,
            channel,
            target_db,
            duration_ms,
            curve,
        } => {
            run_fade_test(
                host,
                port,
                timeout_ms,
                group,
                channel,
                target_db,
                duration_ms,
                curve,
            )
            .await
        }
        Command::Vegas {
            host,
            port,
            timeout_ms,
        } => run_vegas(host, port, timeout_ms).await,
        Command::PanFamilySmokeTest {
            host,
            port,
            timeout_ms,
            log_dir,
            group,
            channel,
        } => {
            run_pan_family_smoke_test(PanFamilySmokeOptions {
                host,
                port,
                timeout_ms,
                log_dir,
                group,
                channel,
            })
            .await
        }
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

fn run_discover(timeout_ms: u64, filter_host: Option<String>, json: bool) -> AppResult<()> {
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

async fn run_listen(
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
    log_dir: PathBuf,
    json: bool,
) -> AppResult<()> {
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("lv1-probe-{}.jsonl", unix_timestamp_secs()));
    let mut logger = JsonlLogger::create(&log_path)?;
    let (host, port) = resolve_target(host, port, timeout_ms)?;
    let mut client = Lv1TcpClient::connect(&host, port).await?;
    client
        .register_myfoh("lv1-probe", &uuid::Uuid::new_v4().to_string())
        .await?;
    eprintln!("listening on {host}:{port}; writing {}", log_path.display());

    loop {
        for frame in client.read_available().await? {
            let msg = decode_frame_payload(&frame)?;
            if let Some((address, args)) = pong_for_ping(&msg) {
                client.send(address, &args).await?;
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

async fn run_set_gain(
    host: Option<String>,
    port: Option<u16>,
    group: i32,
    channel: i32,
    gain_db: f64,
) -> AppResult<()> {
    let (host, port) = resolve_target(host, port, 6000)?;
    let mut client = Lv1TcpClient::connect(&host, port).await?;
    client
        .register_myfoh("lv1-probe", &uuid::Uuid::new_v4().to_string())
        .await?;
    client
        .send(
            "/Set/Track/Out/Gain",
            &[
                OscArg::Int(group),
                OscArg::Int(channel),
                OscArg::Double(gain_db),
            ],
        )
        .await?;

    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        for frame in client.read_available().await? {
            let msg = decode_frame_payload(&frame)?;
            if let Some((address, args)) = pong_for_ping(&msg) {
                client.send(address, &args).await?;
            }
            println!("received {} {:?}", msg.address, msg.args);
        }
    }
    Ok(())
}

struct PanProbeOptions {
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
    log_dir: PathBuf,
    group: i32,
    channel: i32,
    pan: f64,
    width: f64,
    balance: f64,
}

async fn run_pan_probe(options: PanProbeOptions) -> AppResult<()> {
    let PanProbeOptions {
        host,
        port,
        timeout_ms,
        log_dir,
        group,
        channel,
        pan,
        width,
        balance,
    } = options;

    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("lv1-pan-probe-{}.jsonl", unix_timestamp_secs()));
    let mut logger = JsonlLogger::create(&log_path)?;
    let (host, port) = resolve_target(host, port, timeout_ms)?;
    let mut client = Lv1TcpClient::connect(&host, port).await?;
    client
        .register_myfoh("lv1-pan-probe", &uuid::Uuid::new_v4().to_string())
        .await?;
    eprintln!("pan-probe on {host}:{port}; writing {}", log_path.display());
    eprintln!("target group={group} channel={channel}");

    drain_probe_messages(&mut client, &mut logger, Duration::from_secs(2)).await?;

    eprintln!("sending /Set/Track/Pan {pan}");
    client
        .send(
            "/Set/Track/Pan",
            &[
                OscArg::Int(group),
                OscArg::Int(channel),
                OscArg::Double(pan),
            ],
        )
        .await?;
    drain_probe_messages(&mut client, &mut logger, Duration::from_secs(2)).await?;

    eprintln!("sending /Set/Track/Pan/Width {width}");
    client
        .send(
            "/Set/Track/Pan/Width",
            &[
                OscArg::Int(group),
                OscArg::Int(channel),
                OscArg::Double(width),
            ],
        )
        .await?;
    drain_probe_messages(&mut client, &mut logger, Duration::from_secs(2)).await?;

    eprintln!("sending /Set/Track/Pan/Balance {balance}");
    client
        .send(
            "/Set/Track/Pan/Balance",
            &[
                OscArg::Int(group),
                OscArg::Int(channel),
                OscArg::Double(balance),
            ],
        )
        .await?;
    drain_probe_messages(&mut client, &mut logger, Duration::from_secs(2)).await?;

    eprintln!("pan-probe complete");
    Ok(())
}

async fn drain_probe_messages(
    client: &mut Lv1TcpClient,
    logger: &mut JsonlLogger,
    duration: Duration,
) -> AppResult<()> {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        for frame in client.read_available().await? {
            let msg = decode_frame_payload(&frame)?;
            if let Some((address, args)) = pong_for_ping(&msg) {
                client.send(address, &args).await?;
            }
            let entry = entry_for_message(
                "received",
                &msg,
                Some(frame.payload.len()),
                Some(frame.header),
            );
            println!("{} {:?}", msg.address, msg.args);
            logger.write(entry)?;
        }
    }
    Ok(())
}

async fn run_monitor(host: Option<String>, port: Option<u16>, timeout_ms: u64) -> AppResult<()> {
    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = spawn_actor(host.clone(), port, event_bus.clone(), 0);

    loop {
        match events.recv().await {
            Ok(app_event) => {
                let AppEvent::Lv1 { event, .. } = app_event else {
                    continue;
                };

                match event {
                    Lv1Event::Connected => println!("[connected] {host}:{port}"),
                    Lv1Event::Disconnected { reason } => {
                        println!("[disconnected] {reason}; reconnecting in 3s...")
                    }
                    Lv1Event::SceneChanged(scene) => {
                        println!("[scene] index={} name={:?}", scene.index, scene.name);
                    }
                    Lv1Event::SceneListChanged(list) => {
                        println!("[scene-list] {} scenes", list.len());
                        for entry in &list {
                            println!("  [{}] {:?}", entry.index, entry.name);
                        }
                    }
                    Lv1Event::FaderChanged {
                        group,
                        channel,
                        gain_db,
                    } => {
                        println!("[fader] group={group} ch={channel} {gain_db:.1} dB");
                    }
                    Lv1Event::MuteChanged {
                        group,
                        channel,
                        muted,
                    } => {
                        println!("[mute] group={group} ch={channel} muted={muted}");
                    }
                    Lv1Event::PanChanged {
                        group,
                        channel,
                        pan,
                    } => {
                        println!("[pan] group={group} ch={channel} pan={pan:.1}");
                    }
                    Lv1Event::BalanceChanged {
                        group,
                        channel,
                        balance,
                    } => {
                        println!("[pan] group={group} ch={channel} balance={balance:.1}");
                    }
                    Lv1Event::WidthChanged {
                        group,
                        channel,
                        width,
                    } => {
                        println!("[pan] group={group} ch={channel} width={width:.2}");
                    }
                    Lv1Event::ChannelTopologyChanged(channels) => {
                        println!("[channels] {} channels loaded", channels.len());
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                log_lagged_subscriber("monitor", count);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_rate_test(
    host: Option<String>,
    port: Option<u16>,
    group: i32,
    channel: i32,
    rate_hz: u64,
    count: u64,
    start_db: f64,
    end_db: f64,
) -> AppResult<()> {
    let (host, port) = resolve_target(host, port, 6000)?;
    let mut client = Lv1TcpClient::connect(&host, port).await?;
    client
        .register_myfoh("lv1-rate-test", &uuid::Uuid::new_v4().to_string())
        .await?;

    let interval = Duration::from_micros(1_000_000 / rate_hz);
    let step = if count > 1 {
        (end_db - start_db) / (count - 1) as f64
    } else {
        0.0
    };

    eprintln!(
        "rate-test: group={group} ch={channel} {count} cmds @ {rate_hz} Hz ({start_db:.1}→{end_db:.1} dB)"
    );
    eprintln!(
        "interval={:.1}ms  step={:.3} dB",
        interval.as_millis() as f64,
        step
    );

    let mut sent_times: Vec<Instant> = Vec::with_capacity(count as usize);
    let mut echo_times: Vec<(usize, Instant)> = Vec::new();

    for i in 0..count {
        let gain_db = start_db + i as f64 * step;
        let t = Instant::now();
        client
            .send(
                "/Set/Track/Out/Gain",
                &[
                    OscArg::Int(group),
                    OscArg::Int(channel),
                    OscArg::Double(gain_db),
                ],
            )
            .await?;
        sent_times.push(t);

        // Drain any frames that arrived since last send
        for frame in client.read_available().await? {
            if let Ok(msg) = decode_frame_payload(&frame) {
                if let Some((addr, args)) = pong_for_ping(&msg) {
                    client.send(addr, &args).await?;
                } else if msg.address == "/Notify/Track/Out/Gain" {
                    echo_times.push((sent_times.len() - 1, Instant::now()));
                }
            }
        }

        if i + 1 < count {
            tokio::time::sleep(interval).await;
        }
    }

    // Wait up to 2s for remaining echoes
    let wait_until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < wait_until && echo_times.len() < count as usize {
        for frame in client.read_available().await? {
            if let Ok(msg) = decode_frame_payload(&frame) {
                if let Some((addr, args)) = pong_for_ping(&msg) {
                    client.send(addr, &args).await?;
                } else if msg.address == "/Notify/Track/Out/Gain" {
                    echo_times.push((sent_times.len() - 1, Instant::now()));
                }
            }
        }
    }

    let received = echo_times.len();
    println!("Sent:     {count} commands at {rate_hz} Hz");
    println!("Received: {received} echoes");
    println!("Echo rate: {:.1}%", received as f64 / count as f64 * 100.0);

    if !echo_times.is_empty() {
        let latencies_ms: Vec<f64> = echo_times
            .iter()
            .map(|(i, t)| t.duration_since(sent_times[*i]).as_secs_f64() * 1000.0)
            .collect();
        let avg = latencies_ms.iter().sum::<f64>() / latencies_ms.len() as f64;
        let min = latencies_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = latencies_ms
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        println!("Echo latency: avg={avg:.1}ms  min={min:.1}ms  max={max:.1}ms");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_fade_test(
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
    group: i32,
    channel: i32,
    target_db: f64,
    duration_ms: u64,
    curve: CurveArg,
) -> AppResult<()> {
    use advanced_show_control::runtime::commands::AppCommandBus;

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let fade_curve = match curve {
        CurveArg::Linear => FadeCurve::Linear,
    };

    let event_bus = AppEventBus::default();
    let mut lv1_events = event_bus.subscribe();
    let lv1 = spawn_actor(host.clone(), port, event_bus.clone(), 0);
    let command_bus = AppCommandBus::new();
    let engine = spawn_engine(command_bus.clone(), lv1.clone(), event_bus.clone(), 0);
    command_bus.set_fade(Some(engine.clone())).await;
    let mut fade_events = event_bus.subscribe();

    // Wait for LV1 connection
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match lv1_events.recv().await {
                Ok(app_event) => {
                    let AppEvent::Lv1 { event, .. } = app_event else {
                        continue;
                    };

                    if matches!(event, Lv1Event::Connected) {
                        println!("[connected] {host}:{port}");
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("fade-test", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
    .await
    .map_err(|_| "timed out waiting for LV1 connection")?;

    // Wait briefly for /Channels to arrive
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply }).await?;
    let snapshot = rx.await?;
    let current_db = snapshot
        .channels
        .iter()
        .find(|ch| ch.group == group && ch.channel == channel)
        .map(|ch| ch.gain_db);

    match current_db {
        Some(db) => println!(
            "[current] group={group} ch={channel} {db:.1} dB → {target_db:.1} dB over {duration_ms}ms {:?}",
            fade_curve
        ),
        None => println!(
            "[warning] channel group={group} ch={channel} not found in snapshot — fade will start from target"
        ),
    }

    let _ = engine
        .start_fade(FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group,
                channel,
                parameter: FadeParameter::FaderDb,
                target: target_db,
            }],
            duration_ms,
            curve: fade_curve,
        })
        .await;

    loop {
        match fade_events.recv().await {
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::FadeStarted,
            }) => println!("[fade-started]"),
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::FadeCompleted,
            }) => {
                println!("[fade-complete] reached {target_db:.1} dB");
                break;
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::FadeAborted,
            }) => {
                println!("[fade-aborted]");
                break;
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::ChannelOverride { group, channel, .. },
            }) => {
                println!(
                    "[override] group={group} ch={channel} — manual move detected, channel cancelled"
                );
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::ChannelCancelled { group, channel, .. },
            }) => {
                println!("[cancelled] group={group} ch={channel}");
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                log_lagged_subscriber("fade-test", count);
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }

    Ok(())
}

struct PanFamilySmokeOptions {
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: u64,
    log_dir: PathBuf,
    group: i32,
    channel: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PanFamilySmokePosition {
    label: &'static str,
    pan: f64,
    balance: f64,
    width: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct PanFamilySmokeStep {
    stage: &'static str,
    loop_index: usize,
    duration_ms: u64,
    position: PanFamilySmokePosition,
}

const PAN_FAMILY_SMOKE_DURATIONS_MS: [u64; 4] = [5_000, 2_500, 1_000, 100];
const PAN_FAMILY_DEFAULT: PanFamilySmokePosition = PanFamilySmokePosition {
    label: "default",
    pan: 0.0,
    balance: 0.0,
    width: 1.0,
};
const PAN_FAMILY_ALL_MIN: PanFamilySmokePosition = PanFamilySmokePosition {
    label: "min",
    pan: -45.0,
    balance: -45.0,
    width: -1.4,
};
const PAN_FAMILY_ALL_MAX: PanFamilySmokePosition = PanFamilySmokePosition {
    label: "max",
    pan: 45.0,
    balance: 45.0,
    width: 1.4,
};
const PAN_FAMILY_ALTERNATE_A: PanFamilySmokePosition = PanFamilySmokePosition {
    label: "pan_min_balance_max_width_min",
    pan: -45.0,
    balance: 45.0,
    width: -1.4,
};
const PAN_FAMILY_ALTERNATE_B: PanFamilySmokePosition = PanFamilySmokePosition {
    label: "pan_max_balance_min_width_max",
    pan: 45.0,
    balance: -45.0,
    width: 1.4,
};

fn pan_family_smoke_steps() -> Vec<PanFamilySmokeStep> {
    let stages = [
        (
            "together",
            [
                PAN_FAMILY_DEFAULT,
                PAN_FAMILY_ALL_MIN,
                PAN_FAMILY_ALL_MAX,
                PAN_FAMILY_ALL_MIN,
                PAN_FAMILY_ALL_MAX,
                PAN_FAMILY_DEFAULT,
            ],
        ),
        (
            "alternating",
            [
                PAN_FAMILY_DEFAULT,
                PAN_FAMILY_ALTERNATE_A,
                PAN_FAMILY_ALTERNATE_B,
                PAN_FAMILY_ALTERNATE_A,
                PAN_FAMILY_ALTERNATE_B,
                PAN_FAMILY_DEFAULT,
            ],
        ),
    ];

    let mut steps = Vec::new();
    for (stage, positions) in stages {
        for (loop_index, duration_ms) in PAN_FAMILY_SMOKE_DURATIONS_MS.into_iter().enumerate() {
            for position in positions {
                steps.push(PanFamilySmokeStep {
                    stage,
                    loop_index: loop_index + 1,
                    duration_ms,
                    position,
                });
            }
        }
    }
    steps
}

fn pan_family_smoke_config(group: i32, channel: i32, step: &PanFamilySmokeStep) -> FadeConfig {
    FadeConfig {
        scene: FadeSceneIdentity {
            index: (step.loop_index as i32),
            name: format!(
                "pan-family-smoke-{}-loop-{}-{}",
                step.stage, step.loop_index, step.position.label
            ),
        },
        targets: vec![
            FadeTarget {
                group,
                channel,
                parameter: FadeParameter::Pan,
                target: step.position.pan,
            },
            FadeTarget {
                group,
                channel,
                parameter: FadeParameter::Balance,
                target: step.position.balance,
            },
            FadeTarget {
                group,
                channel,
                parameter: FadeParameter::Width,
                target: step.position.width,
            },
        ],
        duration_ms: step.duration_ms,
        curve: FadeCurve::Linear,
    }
}

async fn run_pan_family_smoke_test(options: PanFamilySmokeOptions) -> AppResult<()> {
    use advanced_show_control::runtime::commands::AppCommandBus;

    let PanFamilySmokeOptions {
        host,
        port,
        timeout_ms,
        log_dir,
        group,
        channel,
    } = options;

    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!(
        "lv1-pan-family-smoke-{}.jsonl",
        unix_timestamp_secs()
    ));
    write_smoke_log_entry(
        &log_path,
        serde_json::json!({
            "event": "smoke_start",
            "group": group,
            "channel": channel,
            "durations_ms": PAN_FAMILY_SMOKE_DURATIONS_MS,
        }),
    )?;

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");
    println!("[smoke-log] {}", log_path.display());
    println!("[target] group={group} channel={channel}");

    let event_bus = AppEventBus::default();
    let mut lv1_events = event_bus.subscribe();
    let lv1 = spawn_actor(host.clone(), port, event_bus.clone(), 0);
    let command_bus = AppCommandBus::new();
    let engine = spawn_engine(command_bus.clone(), lv1.clone(), event_bus.clone(), 0);
    command_bus.set_fade(Some(engine.clone())).await;
    let mut fade_events = event_bus.subscribe();

    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match lv1_events.recv().await {
                Ok(AppEvent::Lv1 {
                    generation: 0,
                    event: Lv1Event::Connected,
                }) => {
                    println!("[connected] {host}:{port}");
                    break;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("pan-family-smoke-test", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
    .await
    .map_err(|_| "timed out waiting for LV1 connection")?;

    tokio::time::sleep(Duration::from_millis(300)).await;
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply }).await?;
    let snapshot = rx.await?;
    let channel_found = snapshot
        .channels
        .iter()
        .any(|ch| ch.group == group && ch.channel == channel);
    if channel_found {
        println!("[current] group={group} channel={channel} found in LV1 snapshot");
    } else {
        println!("[warning] group={group} channel={channel} not found in LV1 snapshot");
    }

    let mut failed_loops = Vec::new();
    let steps = pan_family_smoke_steps();
    for stage in ["together", "alternating"] {
        for loop_index in 1..=PAN_FAMILY_SMOKE_DURATIONS_MS.len() {
            let loop_steps: Vec<_> = steps
                .iter()
                .filter(|step| step.stage == stage && step.loop_index == loop_index)
                .collect();
            let mut failed_reason = None;
            for step in loop_steps {
                let label = format!(
                    "{}-loop-{}-{}-{}ms",
                    step.stage, step.loop_index, step.position.label, step.duration_ms
                );
                if let Err(err) = run_pan_family_smoke_step(
                    &engine,
                    &mut fade_events,
                    &log_path,
                    pan_family_smoke_config(group, channel, step),
                    step,
                    &label,
                )
                .await
                {
                    let reason = err.to_string();
                    println!("[loop-failed] {stage} loop {loop_index}: {reason}");
                    write_smoke_log_entry(
                        &log_path,
                        serde_json::json!({
                            "event": "loop_failed",
                            "stage": stage,
                            "loop_index": loop_index,
                            "reason": reason,
                        }),
                    )?;
                    let _ = engine.abort_all().await;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    drain_pending_smoke_events(&mut fade_events);
                    failed_reason = Some(reason);
                    break;
                }
            }

            if let Some(reason) = failed_reason {
                failed_loops.push((stage.to_string(), loop_index, reason));
            } else {
                println!("[loop-complete] {stage} loop {loop_index}");
                write_smoke_log_entry(
                    &log_path,
                    serde_json::json!({
                        "event": "loop_completed",
                        "stage": stage,
                        "loop_index": loop_index,
                    }),
                )?;
            }
        }
    }

    write_smoke_log_entry(
        &log_path,
        serde_json::json!({
            "event": "smoke_complete",
            "failed_loop_count": failed_loops.len(),
        }),
    )?;
    if !failed_loops.is_empty() {
        println!("[smoke-complete] {} loop(s) failed", failed_loops.len());
        println!("[smoke-log] {}", log_path.display());
        return Err(format!(
            "pan-family smoke test failed {} loop(s); inspect smoke log: {}",
            failed_loops.len(),
            log_path.display()
        )
        .into());
    }

    println!("[smoke-complete] all loops passed with no fade override detected");
    println!("[smoke-log] {}", log_path.display());
    Ok(())
}

async fn run_pan_family_smoke_step(
    engine: &FadeEngineHandle,
    fade_events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    log_path: &std::path::Path,
    config: FadeConfig,
    step: &PanFamilySmokeStep,
    label: &str,
) -> AppResult<()> {
    drain_pending_smoke_events(fade_events);

    let targets: Vec<_> = config
        .targets
        .iter()
        .map(|target| {
            serde_json::json!({
                "group": target.group,
                "channel": target.channel,
                "parameter": format!("{:?}", target.parameter),
                "target": target.target,
            })
        })
        .collect();

    println!("[fade-start] {label}");
    write_smoke_log_entry(
        log_path,
        serde_json::json!({
            "event": "fade_request",
            "label": label,
            "stage": step.stage,
            "loop_index": step.loop_index,
            "position": step.position.label,
            "duration_ms": config.duration_ms,
            "targets": targets,
        }),
    )?;

    engine.start_fade(config).await?;

    loop {
        match fade_events.recv().await {
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::FadeStarted,
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "fade_started",
                        "label": label,
                        "stage": step.stage,
                        "loop_index": step.loop_index,
                        "position": step.position.label,
                    }),
                )?;
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::FadeCompleted,
            }) => {
                println!("[fade-complete] {label}");
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "fade_completed",
                        "label": label,
                        "stage": step.stage,
                        "loop_index": step.loop_index,
                        "position": step.position.label,
                    }),
                )?;
                break;
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::FadeAborted,
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "fade_aborted",
                        "label": label,
                        "stage": step.stage,
                        "loop_index": step.loop_index,
                        "position": step.position.label,
                    }),
                )?;
                return Err(format!(
                    "pan-family smoke test fade aborted during {label}; smoke log: {}",
                    log_path.display()
                )
                .into());
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event:
                    FadeEvent::ChannelOverride {
                        group,
                        channel,
                        parameter,
                    },
            }) => {
                println!(
                    "[override] group={group} ch={channel} parameter={parameter:?}; stopping smoke test"
                );
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "manual_override_detected",
                        "label": label,
                        "group": group,
                        "channel": channel,
                        "parameter": format!("{:?}", parameter),
                    }),
                )?;
                return Err(format!(
                    "manual override detected during pan-family smoke test; inspect smoke log: {}",
                    log_path.display()
                )
                .into());
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event:
                    FadeEvent::ChannelCancelled {
                        group,
                        channel,
                        parameter,
                    },
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "channel_cancelled",
                        "label": label,
                        "group": group,
                        "channel": channel,
                        "parameter": format!("{:?}", parameter),
                    }),
                )?;
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event:
                    FadeEvent::ChannelCompleted {
                        group,
                        channel,
                        parameter,
                    },
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "channel_completed",
                        "label": label,
                        "group": group,
                        "channel": channel,
                        "parameter": format!("{:?}", parameter),
                    }),
                )?;
            }
            Ok(AppEvent::Fade {
                generation: 0,
                event: FadeEvent::WriteFailed { reason },
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({ "event": "write_failed", "label": label, "reason": reason }),
                )?;
                return Err(format!(
                    "pan-family smoke test write failed during {label}: {reason}; smoke log: {}",
                    log_path.display()
                )
                .into());
            }
            Ok(AppEvent::Lv1 {
                generation: 0,
                event:
                    Lv1Event::PanChanged {
                        group,
                        channel,
                        pan,
                    },
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "lv1_pan_changed",
                        "label": label,
                        "group": group,
                        "channel": channel,
                        "value": pan,
                    }),
                )?;
            }
            Ok(AppEvent::Lv1 {
                generation: 0,
                event:
                    Lv1Event::BalanceChanged {
                        group,
                        channel,
                        balance,
                    },
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "lv1_balance_changed",
                        "label": label,
                        "group": group,
                        "channel": channel,
                        "value": balance,
                    }),
                )?;
            }
            Ok(AppEvent::Lv1 {
                generation: 0,
                event:
                    Lv1Event::WidthChanged {
                        group,
                        channel,
                        width,
                    },
            }) => {
                write_smoke_log_entry(
                    log_path,
                    serde_json::json!({
                        "event": "lv1_width_changed",
                        "label": label,
                        "group": group,
                        "channel": channel,
                        "value": width,
                    }),
                )?;
            }
            Ok(_) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                log_lagged_subscriber("pan-family-smoke-test", count);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err("pan-family smoke test event bus closed".into());
            }
        }
    }

    Ok(())
}

fn drain_pending_smoke_events(fade_events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    while fade_events.try_recv().is_ok() {}
}

fn write_smoke_log_entry(path: &std::path::Path, mut entry: serde_json::Value) -> AppResult<()> {
    use std::io::Write;

    if let Some(object) = entry.as_object_mut() {
        object.insert(
            "timestamp_unix_secs".to_string(),
            serde_json::json!(unix_timestamp_secs()),
        );
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}

async fn wait_for_channels_until(
    lv1: &Lv1ActorHandle,
    deadline: Instant,
) -> AppResult<Vec<ChannelInfo>> {
    loop {
        let (reply, rx) = oneshot::channel();
        lv1.send(Lv1Command::GetState { reply }).await?;
        let snapshot = rx.await?;
        if !snapshot.channels.is_empty() {
            return Ok(snapshot.channels);
        }
        let now = Instant::now();
        if now >= deadline {
            return Err("timed out waiting for LV1 channel snapshot".into());
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(50))).await;
    }
}

// LV1 has no explicit mute-list-complete marker, so quiet settling is the best
// protocol boundary we have for the initial Vegas mute baseline.
const INITIAL_MUTE_SETTLE_MS: u64 = 150;

async fn wait_for_channels_with_mute_settle(
    lv1: &Lv1ActorHandle,
    _event_bus: &AppEventBus,
    events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
    timeout_ms: u64,
) -> AppResult<Vec<ChannelInfo>> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut snapshot = wait_for_channels_until(lv1, deadline).await?;
    let settle_window = Duration::from_millis(INITIAL_MUTE_SETTLE_MS);
    let mut settle_deadline = Instant::now() + settle_window;

    loop {
        let now = Instant::now();
        if now >= deadline {
            return Ok(snapshot);
        }

        let sleep_until = settle_deadline.min(deadline);
        let sleep = tokio::time::sleep(sleep_until.saturating_duration_since(now));
        tokio::pin!(sleep);

        tokio::select! {
            _ = &mut sleep => {
                let (reply, rx) = oneshot::channel();
                lv1.send(Lv1Command::GetState { reply }).await?;
                let latest = rx.await?;
                if !latest.channels.is_empty() {
                    snapshot = latest.channels;
                }
                if Instant::now() >= settle_deadline || Instant::now() >= deadline {
                    return Ok(snapshot);
                }
            }
            event = events.recv() => {
                match event {
                    Ok(app_event) => {
                        let AppEvent::Lv1 { event, .. } = app_event else {
                            continue;
                        };

                        if matches!(event, Lv1Event::MuteChanged { .. } | Lv1Event::ChannelTopologyChanged(_)) {
                            let (reply, rx) = oneshot::channel();
                            lv1.send(Lv1Command::GetState { reply }).await?;
                            let latest = rx.await?;
                            if !latest.channels.is_empty() {
                                snapshot = latest.channels;
                            }
                            settle_deadline = Instant::now() + settle_window;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("vegas-settle", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {}
                }
            }
        }
    }
}

fn summarize_vegas_restore_failures(failures: &[String]) -> String {
    format!("failed to restore Vegas snapshot: {}", failures.join("; "))
}

async fn restore_vegas_snapshot(lv1: &Lv1ActorHandle, original: &[ChannelInfo]) -> AppResult<()> {
    let mut failures = Vec::new();

    for ch in original {
        let (reply, rx) = oneshot::channel();
        if let Err(err) = lv1
            .send(Lv1Command::SetGain {
                group: ch.group,
                channel: ch.channel,
                gain_db: ch.gain_db,
                reply,
            })
            .await
        {
            failures.push(format!(
                "gain restore failed for {}:{} ({err})",
                ch.group, ch.channel
            ));
            continue;
        }
        if let Err(err) = rx.await? {
            failures.push(format!(
                "gain restore failed for {}:{} ({err})",
                ch.group, ch.channel
            ));
        }
    }
    let (reply, rx) = oneshot::channel();
    if let Err(err) = lv1.send(Lv1Command::Flush { reply }).await {
        failures.push(format!("gain flush failed ({err})"));
    }
    if let Ok(result) = rx.await
        && let Err(err) = result
    {
        failures.push(format!("gain flush failed ({err})"));
    }

    for ch in original {
        let (reply, rx) = oneshot::channel();
        if let Err(err) = lv1
            .send(Lv1Command::SetMute {
                group: ch.group,
                channel: ch.channel,
                muted: ch.muted,
                reply,
            })
            .await
        {
            failures.push(format!(
                "mute restore failed for {}:{} ({err})",
                ch.group, ch.channel
            ));
            continue;
        }
        if let Err(err) = rx.await? {
            failures.push(format!(
                "mute restore failed for {}:{} ({err})",
                ch.group, ch.channel
            ));
        }
    }
    let (reply, rx) = oneshot::channel();
    if let Err(err) = lv1.send(Lv1Command::Flush { reply }).await {
        failures.push(format!("mute flush failed ({err})"));
    }
    if let Ok(result) = rx.await
        && let Err(err) = result
    {
        failures.push(format!("mute flush failed ({err})"));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(summarize_vegas_restore_failures(&failures).into())
    }
}

async fn run_vegas(host: Option<String>, port: Option<u16>, timeout_ms: u64) -> AppResult<()> {
    use advanced_show_control::vegas::gain_db_at;

    const VEGAS_TICK_HZ: u64 = 25;

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let lv1 = spawn_actor(host.clone(), port, event_bus.clone(), 0);

    tokio::time::timeout(Duration::from_millis(timeout_ms), async {
        loop {
            match events.recv().await {
                Ok(app_event) => {
                    let AppEvent::Lv1 { event, .. } = app_event else {
                        continue;
                    };

                    if matches!(event, Lv1Event::Connected) {
                        println!("[connected] {host}:{port}");
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("vegas", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
    .await
    .map_err(|_| "timed out waiting for LV1 connection")?;

    let mut original: Vec<ChannelInfo> =
        wait_for_channels_with_mute_settle(&lv1, &event_bus, &mut events, timeout_ms).await?;
    original.sort_by_key(|ch| (ch.group, ch.channel));
    println!("[vegas] captured {} faders", original.len());

    for ch in &original {
        let (reply, rx) = oneshot::channel();
        lv1.send(Lv1Command::SetMute {
            group: ch.group,
            channel: ch.channel,
            muted: true,
            reply,
        })
        .await?;
        rx.await??;
    }
    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::Flush { reply }).await?;
    rx.await??;
    println!("[vegas] muted captured faders; press Ctrl-C to stop and restore");

    let mut interval = tokio::time::interval(Duration::from_millis(1000 / VEGAS_TICK_HZ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut tick = 0_u64;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("[vegas] stopping; restoring captured faders");
                break;
            }
            _ = interval.tick() => {
                for ch in &original {
                    let (reply, rx) = oneshot::channel();
                    let result: AppResult<()> = match lv1.send(Lv1Command::SetGain {
                        group: ch.group,
                        channel: ch.channel,
                        gain_db: gain_db_at(ch.group, ch.channel, tick),
                        reply,
                    }).await {
                        Ok(()) => match rx.await {
                            Ok(result) => result.map_err(|err| err.into()),
                            Err(err) => Err(err.into()),
                        },
                        Err(err) => Err(err.into()),
                    };
                    if let Err(err) = result {
                        let animation_error = format!("[vegas] animation failed for {}:{} ({err})", ch.group, ch.channel);
                        let restore_error = restore_vegas_snapshot(&lv1, &original).await.err();
                        return match restore_error {
                            Some(restore_error) => Err(format!("{animation_error}; {restore_error}").into()),
                            None => Err(animation_error.into()),
                        };
                    }
                }
                tick = tick.wrapping_add(1);
            }
        }
    }

    let restore_result = restore_vegas_snapshot(&lv1, &original).await;
    if restore_result.is_ok() {
        println!("[vegas] restore commands sent");
    }
    restore_result
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
    use std::io::Write;
    use std::net::TcpListener;
    use std::time::Duration;

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
    fn parses_discover_command_for_probe_binary() {
        let cli = parse_cli_from(["lv1-probe", "discover", "--timeout-ms", "100", "--json"])
            .expect("discover command should parse");

        match cli.command {
            Command::Discover {
                timeout_ms,
                filter_host,
                json,
            } => {
                assert_eq!(timeout_ms, 100);
                assert_eq!(filter_host, None);
                assert!(json);
            }
            other => panic!("expected discover command, got {other:?}"),
        }
    }

    #[test]
    fn help_uses_lv1_probe_name_even_when_binary_name_differs() {
        let err = parse_cli_from(["advanced-show-control", "--help"]).unwrap_err();

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
            Command::Monitor {
                host,
                port,
                timeout_ms,
            } => {
                assert_eq!(host.as_deref(), Some("192.168.1.10"));
                assert_eq!(port, Some(50000));
                assert_eq!(timeout_ms, 3000);
            }
            other => panic!("expected monitor command, got {other:?}"),
        }
    }

    #[test]
    fn parses_fade_test_command() {
        let cli = Cli::try_parse_from([
            "lv1-probe",
            "fade-test",
            "--host",
            "192.168.1.10",
            "--port",
            "50001",
            "--group",
            "0",
            "--channel",
            "2",
            "--target-db",
            "-20.0",
            "--duration-ms",
            "3000",
            "--curve",
            "linear",
        ])
        .unwrap();

        match cli.command {
            Command::FadeTest {
                host,
                port,
                group,
                channel,
                target_db,
                duration_ms,
                curve,
                ..
            } => {
                assert_eq!(host.as_deref(), Some("192.168.1.10"));
                assert_eq!(port, Some(50001));
                assert_eq!(group, 0);
                assert_eq!(channel, 2);
                assert!((target_db - -20.0).abs() < 1e-10);
                assert_eq!(duration_ms, 3000);
                assert!(matches!(curve, CurveArg::Linear));
            }
            other => panic!("expected FadeTest, got {other:?}"),
        }
    }

    #[test]
    fn parses_vegas_command_without_group_option() {
        let cli = Cli::try_parse_from([
            "lv1-probe",
            "vegas",
            "--host",
            "192.168.1.10",
            "--port",
            "50001",
            "--timeout-ms",
            "3000",
        ])
        .unwrap();

        match cli.command {
            Command::Vegas {
                host,
                port,
                timeout_ms,
            } => {
                assert_eq!(host.as_deref(), Some("192.168.1.10"));
                assert_eq!(port, Some(50001));
                assert_eq!(timeout_ms, 3000);
            }
            other => panic!("expected Vegas, got {other:?}"),
        }

        let err = Cli::try_parse_from(["lv1-probe", "vegas", "--group", "0"]).unwrap_err();
        assert!(err.to_string().contains("unexpected argument '--group'"));
    }

    #[test]
    fn parses_pan_family_smoke_test_command() {
        let cli = Cli::try_parse_from([
            "lv1-probe",
            "pan-family-smoke-test",
            "--host",
            "192.168.1.10",
            "--port",
            "50001",
            "--timeout-ms",
            "3000",
            "--log-dir",
            "logs-smoke",
            "--group",
            "0",
            "--channel",
            "2",
        ])
        .unwrap();

        match cli.command {
            Command::PanFamilySmokeTest {
                host,
                port,
                timeout_ms,
                log_dir,
                group,
                channel,
            } => {
                assert_eq!(host.as_deref(), Some("192.168.1.10"));
                assert_eq!(port, Some(50001));
                assert_eq!(timeout_ms, 3000);
                assert_eq!(log_dir, std::path::PathBuf::from("logs-smoke"));
                assert_eq!(group, 0);
                assert_eq!(channel, 2);
            }
            other => panic!("expected PanFamilySmokeTest, got {other:?}"),
        }
    }

    #[test]
    fn pan_family_smoke_steps_cover_stages_loops_and_positions() {
        let steps = pan_family_smoke_steps();

        assert_eq!(steps.len(), 48);

        let first_loop: Vec<_> = steps
            .iter()
            .filter(|step| step.stage == "together" && step.loop_index == 1)
            .collect();
        assert_eq!(first_loop.len(), 6);
        assert!(first_loop.iter().all(|step| step.duration_ms == 5_000));
        assert_eq!(first_loop[0].position, PAN_FAMILY_DEFAULT);
        assert_eq!(first_loop[1].position, PAN_FAMILY_ALL_MIN);
        assert_eq!(first_loop[2].position, PAN_FAMILY_ALL_MAX);
        assert_eq!(first_loop[3].position, PAN_FAMILY_ALL_MIN);
        assert_eq!(first_loop[4].position, PAN_FAMILY_ALL_MAX);
        assert_eq!(first_loop[5].position, PAN_FAMILY_DEFAULT);

        let alternating_fast_loop: Vec<_> = steps
            .iter()
            .filter(|step| step.stage == "alternating" && step.loop_index == 4)
            .collect();
        assert_eq!(alternating_fast_loop.len(), 6);
        assert!(
            alternating_fast_loop
                .iter()
                .all(|step| step.duration_ms == 100)
        );
        assert_eq!(alternating_fast_loop[0].position, PAN_FAMILY_DEFAULT);
        assert_eq!(alternating_fast_loop[1].position, PAN_FAMILY_ALTERNATE_A);
        assert_eq!(alternating_fast_loop[2].position, PAN_FAMILY_ALTERNATE_B);
        assert_eq!(alternating_fast_loop[3].position, PAN_FAMILY_ALTERNATE_A);
        assert_eq!(alternating_fast_loop[4].position, PAN_FAMILY_ALTERNATE_B);
        assert_eq!(alternating_fast_loop[5].position, PAN_FAMILY_DEFAULT);
    }

    #[tokio::test]
    async fn wait_for_channels_returns_snapshot_after_channels_arrive() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_millis(150));

            let mut args = vec![advanced_show_control::osc::OscArg::Int(1)];
            args.push(advanced_show_control::osc::OscArg::String(
                "Channel 1".to_string(),
            ));
            args.push(advanced_show_control::osc::OscArg::Int(0));
            args.push(advanced_show_control::osc::OscArg::Int(0));
            args.push(advanced_show_control::osc::OscArg::Double(-9.1));
            for _ in 0..15 {
                args.push(advanced_show_control::osc::OscArg::Int(0));
            }

            let frame = advanced_show_control::lv1::encode_frame("/Channels", &args).unwrap();
            stream.write_all(&frame).unwrap();
            std::thread::sleep(Duration::from_millis(250));
        });

        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match events.recv().await {
                    Ok(app_event) => {
                        let AppEvent::Lv1 { event, .. } = app_event else {
                            continue;
                        };

                        if matches!(event, Lv1Event::Connected) {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("vegas-test", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
        .await
        .unwrap();

        let snapshot = wait_for_channels_until(&handle, Instant::now() + Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].name, "Channel 1");

        server.await.unwrap();
    }

    #[tokio::test]
    async fn vegas_channel_snapshot_wait_includes_late_mute_notification() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let channels = vec![
                advanced_show_control::osc::OscArg::Int(1),
                advanced_show_control::osc::OscArg::String("Channel 1".to_string()),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Double(-9.1),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
            ];
            let frame = advanced_show_control::lv1::encode_frame("/Channels", &channels).unwrap();
            stream.write_all(&frame).unwrap();

            std::thread::sleep(Duration::from_millis(50));

            let mute_on = vec![
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Bool(true),
            ];
            let frame =
                advanced_show_control::lv1::encode_frame("/Notify/Track/Out/Mute", &mute_on)
                    .unwrap();
            stream.write_all(&frame).unwrap();

            std::thread::sleep(Duration::from_millis(80));

            let mute_off = vec![
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Int(0),
                advanced_show_control::osc::OscArg::Bool(false),
            ];
            let frame =
                advanced_show_control::lv1::encode_frame("/Notify/Track/Out/Mute", &mute_off)
                    .unwrap();
            stream.write_all(&frame).unwrap();

            std::thread::sleep(Duration::from_millis(250));
        });

        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone(), 0);

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match events.recv().await {
                    Ok(app_event) => {
                        let AppEvent::Lv1 { event, .. } = app_event else {
                            continue;
                        };

                        if matches!(event, Lv1Event::Connected) {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("vegas-test", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
        .await
        .unwrap();

        let snapshot = wait_for_channels_with_mute_settle(&handle, &event_bus, &mut events, 2_000)
            .await
            .unwrap();
        assert_eq!(snapshot.len(), 1);
        assert!(!snapshot[0].muted);

        server.await.unwrap();
    }

    #[test]
    fn summarize_vegas_restore_failures_combines_all_messages() {
        let message = summarize_vegas_restore_failures(&[
            "gain restore failed for 1:2 (boom)".to_string(),
            "mute flush failed (kaput)".to_string(),
        ]);

        assert_eq!(
            message,
            "failed to restore Vegas snapshot: gain restore failed for 1:2 (boom); mute flush failed (kaput)"
        );
    }
}

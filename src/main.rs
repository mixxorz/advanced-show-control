use clap::{Parser, Subcommand};
use lv1_scene_fade_utility::lv1::discovery::{DiscoverOptions, discover, resolve_target};
use lv1_scene_fade_utility::lv1::messages::Lv1Event;
use lv1_scene_fade_utility::lv1::model::ChannelInfo;
use lv1_scene_fade_utility::lv1::probe::{JsonlLogger, MessageKind, entry_for_message};
use lv1_scene_fade_utility::lv1::state::{Lv1ActorHandle, spawn_actor};
use lv1_scene_fade_utility::lv1::tcp::{Lv1TcpClient, decode_frame_payload, pong_for_ping};
use lv1_scene_fade_utility::runtime::events::{AppEvent, AppEventBus, log_lagged_subscriber};
use lv1_scene_fade_utility::osc::OscArg;
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

async fn run_monitor(host: Option<String>, port: Option<u16>, timeout_ms: u64) -> AppResult<()> {
    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let _handle = spawn_actor(host.clone(), port, event_bus.clone());

    loop {
        match events.recv().await {
            Ok(app_event) => {
                let AppEvent::Lv1(event) = app_event else {
                    continue;
                };

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
    use lv1_scene_fade_utility::fade::curve::FadeCurve;
    use lv1_scene_fade_utility::fade::engine::spawn_engine;
    use lv1_scene_fade_utility::fade::types::{FadeConfig, FadeEvent, FadeTarget};
    use lv1_scene_fade_utility::runtime::commands::AppCommandBus;

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let fade_curve = match curve {
        CurveArg::Linear => FadeCurve::Linear,
    };

    let event_bus = AppEventBus::default();
    let mut lv1_events = event_bus.subscribe();
    let lv1 = spawn_actor(host.clone(), port, event_bus.clone());
    let command_bus = AppCommandBus::new(event_bus.clone());
    command_bus.set_lv1(Some(lv1.clone())).await;
    let engine = spawn_engine(command_bus.clone(), event_bus.clone());
    command_bus.set_fade(Some(engine.clone())).await;
    let mut fade_events = event_bus.subscribe();

    // Wait for LV1 connection
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
    loop {
        match lv1_events.recv().await {
            Ok(app_event) => {
                let AppEvent::Lv1(event) = app_event else {
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

    let snapshot = lv1.get_state().await;
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
            targets: vec![FadeTarget {
                group,
                channel,
                target_db,
            }],
            duration_ms,
            curve: fade_curve,
        })
        .await;

    loop {
        match fade_events.recv().await {
            Ok(AppEvent::Fade(FadeEvent::FadeStarted)) => println!("[fade-started]"),
            Ok(AppEvent::Fade(FadeEvent::FadeCompleted)) => {
                println!("[fade-complete] reached {target_db:.1} dB");
                break;
            }
            Ok(AppEvent::Fade(FadeEvent::FadeAborted)) => {
                println!("[fade-aborted]");
                break;
            }
            Ok(AppEvent::Fade(FadeEvent::ChannelOverride { group, channel })) => {
                println!(
                    "[override] group={group} ch={channel} — manual move detected, channel cancelled"
                );
            }
            Ok(AppEvent::Fade(FadeEvent::ChannelCancelled { group, channel })) => {
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

async fn wait_for_channels_until(
    lv1: &Lv1ActorHandle,
    deadline: Instant,
) -> AppResult<Vec<ChannelInfo>> {
    loop {
        let snapshot = lv1.get_state().await;
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
                let latest = lv1.get_state().await;
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
                        let AppEvent::Lv1(event) = app_event else {
                            continue;
                        };

                        if matches!(event, Lv1Event::MuteChanged { .. } | Lv1Event::ChannelTopologyChanged(_)) {
                            let latest = lv1.get_state().await;
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
        if let Err(err) = lv1.set_gain(ch.group, ch.channel, ch.gain_db).await {
            failures.push(format!(
                "gain restore failed for {}:{} ({err})",
                ch.group, ch.channel
            ));
        }
    }
    if let Err(err) = lv1.flush().await {
        failures.push(format!("gain flush failed ({err})"));
    }

    for ch in original {
        if let Err(err) = lv1.set_mute(ch.group, ch.channel, ch.muted).await {
            failures.push(format!(
                "mute restore failed for {}:{} ({err})",
                ch.group, ch.channel
            ));
        }
    }
    if let Err(err) = lv1.flush().await {
        failures.push(format!("mute flush failed ({err})"));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(summarize_vegas_restore_failures(&failures).into())
    }
}

async fn run_vegas(host: Option<String>, port: Option<u16>, timeout_ms: u64) -> AppResult<()> {
    use lv1_scene_fade_utility::vegas::gain_db_at;

    const VEGAS_TICK_HZ: u64 = 25;

    let (host, port) = resolve_target(host, port, timeout_ms)?;
    eprintln!("connecting to {host}:{port}");

    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let lv1 = spawn_actor(host.clone(), port, event_bus.clone());

    tokio::time::timeout(Duration::from_millis(timeout_ms), async {
        loop {
            match events.recv().await {
                Ok(app_event) => {
                    let AppEvent::Lv1(event) = app_event else {
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
        wait_for_channels_with_mute_settle(&lv1, &mut events, timeout_ms).await?;
    original.sort_by_key(|ch| (ch.group, ch.channel));
    println!("[vegas] captured {} faders", original.len());

    for ch in &original {
        lv1.set_mute(ch.group, ch.channel, true).await?;
    }
    lv1.flush().await?;
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
                    if let Err(err) = lv1.set_gain(ch.group, ch.channel, gain_db_at(ch.group, ch.channel, tick)).await {
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

    #[tokio::test]
    async fn wait_for_channels_returns_snapshot_after_channels_arrive() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = tokio::task::spawn_blocking(move || {
            let (mut stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_millis(150));

            let mut args = vec![lv1_scene_fade_utility::osc::OscArg::Int(1)];
            args.push(lv1_scene_fade_utility::osc::OscArg::String(
                "Channel 1".to_string(),
            ));
            args.push(lv1_scene_fade_utility::osc::OscArg::Int(0));
            args.push(lv1_scene_fade_utility::osc::OscArg::Int(0));
            args.push(lv1_scene_fade_utility::osc::OscArg::Double(-9.1));
            for _ in 0..15 {
                args.push(lv1_scene_fade_utility::osc::OscArg::Int(0));
            }

            let frame = lv1_scene_fade_utility::lv1::tcp::encode_frame("/Channels", &args).unwrap();
            stream.write_all(&frame).unwrap();
            std::thread::sleep(Duration::from_millis(250));
        });

        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match events.recv().await {
                    Ok(app_event) => {
                        let AppEvent::Lv1(event) = app_event else {
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
                lv1_scene_fade_utility::osc::OscArg::Int(1),
                lv1_scene_fade_utility::osc::OscArg::String("Channel 1".to_string()),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Double(-9.1),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
            ];
            let frame =
                lv1_scene_fade_utility::lv1::tcp::encode_frame("/Channels", &channels).unwrap();
            stream.write_all(&frame).unwrap();

            std::thread::sleep(Duration::from_millis(50));

            let mute_on = vec![
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Bool(true),
            ];
            let frame =
                lv1_scene_fade_utility::lv1::tcp::encode_frame("/Notify/Track/Out/Mute", &mute_on)
                    .unwrap();
            stream.write_all(&frame).unwrap();

            std::thread::sleep(Duration::from_millis(80));

            let mute_off = vec![
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Int(0),
                lv1_scene_fade_utility::osc::OscArg::Bool(false),
            ];
            let frame =
                lv1_scene_fade_utility::lv1::tcp::encode_frame("/Notify/Track/Out/Mute", &mute_off)
                    .unwrap();
            stream.write_all(&frame).unwrap();

            std::thread::sleep(Duration::from_millis(250));
        });

        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let handle = spawn_actor("127.0.0.1".to_string(), port, event_bus.clone());

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match events.recv().await {
                    Ok(app_event) => {
                        let AppEvent::Lv1(event) = app_event else {
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

        let snapshot = wait_for_channels_with_mute_settle(&handle, &mut events, 2_000)
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

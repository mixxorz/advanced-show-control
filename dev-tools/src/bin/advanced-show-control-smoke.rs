use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use advanced_show_control::application::ApplicationCommandContext;
use advanced_show_control::native_ui::{NativeRuntime, app_config_dir};
use advanced_show_control::projector::{AppConnectionState, AppViewState, ProjectionSubscription};
use advanced_show_control::scenes::{SceneConfig, SceneScopeToggles};
use advanced_show_control::show::{
    SHOW_FILE_SCHEMA_VERSION, ShowFile, ShowFileSafety, ShowFileSceneConfig,
};
use advanced_show_control_dev_tools::smoke::{SmokeReport, SuiteResult, repo_root};
use clap::Parser;
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(15);
const SMOKE_A: &str = "Smoke A";
const SMOKE_B: &str = "Smoke B";
const GROUP: i32 = 0;
const CHANNEL: i32 = 1;
const TARGET_A: f64 = -10.0;
const TARGET_B: f64 = 0.0;
const GAIN_TOLERANCE: f64 = 0.5;
const SAME_SCENE_DURATION: Duration = Duration::from_secs(6);
const SMOKE_APP_IDENTIFIER: &str = "com.advancedshowcontrol.debug";

/// @cc [owner:mixxorz,label:safety] smoke-config-isolation
/// Hardware smoke runs MUST use the debug application identity and MUST NOT read or write the
/// production application's configuration directory.
fn smoke_config_dir() -> PathBuf {
    app_config_dir().with_file_name(SMOKE_APP_IDENTIFIER)
}

#[derive(Parser)]
#[command(about = "Run the non-GUI LV1 hardware smoke suite")]
struct Args {
    #[arg(long, default_value_t = 5_000)]
    discovery_timeout_ms: u64,
}

fn main() {
    let args = Args::parse();
    let root = repo_root();
    let mut report = match SmokeReport::create(&root) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("failed to initialize authoritative smoke report: {error}");
            std::process::exit(2);
        }
    };
    println!("Authoritative report: {}", report.path().display());
    let _ = report.line("START");

    let mut runtime = match NativeRuntime::build_in(smoke_config_dir()) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = report.line(format!("ERROR runtime setup failed: {error:#}"));
            let _ = report.line("SUITE FAIL");
            std::process::exit(1);
        }
    };
    let commands = runtime.commands();
    let debug_commands = runtime.debug_commands();
    let projections = runtime.take_projections();
    let mut suite = runtime.handle().block_on(run_lifecycle(
        commands,
        debug_commands,
        projections,
        &mut report,
        &root,
        args,
    ));
    let shutdown_succeeded = runtime.shutdown();
    let passed = finish_report(&mut report, &mut suite, shutdown_succeeded);

    std::process::exit(if passed { 0 } else { 1 });
}

async fn run_lifecycle(
    commands: ApplicationCommandContext,
    debug_commands: advanced_show_control::debug_tools::DebugRuntimeCommands,
    projections: ProjectionSubscription,
    report: &mut SmokeReport,
    root: &Path,
    args: Args,
) -> SuiteResult {
    let mut suite = SuiteResult::default();
    let mut runner = Runner {
        commands: commands.clone(),
        debug_commands,
        projections,
        report,
        root: root.to_path_buf(),
        scene_a: Uuid::nil(),
        scene_b: Uuid::nil(),
    };

    let initial_settings = match runner.wait_state("initial app state", |_| true).await {
        Ok(state) => Some(state.settings),
        Err(error) => {
            suite.fail(error);
            None
        }
    };
    if suite.is_ok()
        && let Err(error) = runner.run(args.discovery_timeout_ms).await
    {
        suite.fail(error);
    }

    if let Err(error) = commands.set_lockout(false).await {
        suite.fail(format!("lockout cleanup failed: {error}"));
    }
    if let Some(settings) = initial_settings
        && let Err(error) = commands.replace_app_settings(settings).await
    {
        suite.fail(format!("settings cleanup failed: {error}"));
    }

    suite
}

fn finish_report(
    report: &mut SmokeReport,
    suite: &mut SuiteResult,
    shutdown_succeeded: bool,
) -> bool {
    const SHUTDOWN_FAILURE: &str = "runtime shutdown cleanup failed";
    if !shutdown_succeeded {
        if suite
            .first_failure()
            .is_some_and(|error| error != SHUTDOWN_FAILURE)
            && let Err(error) = report.line(format!("ERROR {SHUTDOWN_FAILURE}"))
        {
            eprintln!("failed to report runtime shutdown failure: {error}");
            return false;
        }
        suite.fail(SHUTDOWN_FAILURE);
    }
    if let Some(error) = suite.first_failure()
        && let Err(write_error) = report.line(format!("ERROR {error}"))
    {
        eprintln!("failed to write authoritative suite failure: {write_error}");
        return false;
    }
    let final_line = if suite.is_ok() {
        "SUITE PASS"
    } else {
        "SUITE FAIL"
    };
    if let Err(error) = report.line(final_line) {
        eprintln!("failed to write authoritative suite result: {error}");
        return false;
    }
    suite.is_ok()
}

struct Runner<'a> {
    commands: ApplicationCommandContext,
    debug_commands: advanced_show_control::debug_tools::DebugRuntimeCommands,
    projections: ProjectionSubscription,
    report: &'a mut SmokeReport,
    root: PathBuf,
    scene_a: Uuid,
    scene_b: Uuid,
}

impl Runner<'_> {
    async fn run(&mut self, discovery_timeout_ms: u64) -> Result<(), String> {
        self.test("cue-list-create", |this| Box::pin(this.cue_list_create()))
            .await?;
        self.test("connection", |this| {
            Box::pin(this.connection(discovery_timeout_ms))
        })
        .await?;
        self.test("startup-auto-connect", |this| {
            Box::pin(this.startup_auto_connect())
        })
        .await?;
        self.test("empty-scene-settings-defaults", |this| {
            Box::pin(this.empty_defaults())
        })
        .await?;
        self.test("scene-settings-copy-paste", |this| {
            Box::pin(this.copy_paste())
        })
        .await?;
        self.test("new-session-clears-scene-settings-clipboard", |this| {
            Box::pin(this.clipboard_reset())
        })
        .await?;
        self.test("scene-recall", |this| Box::pin(this.scene_recall()))
            .await?;
        self.test("rapid-scene-recall-queue", |this| {
            Box::pin(this.rapid_recall())
        })
        .await?;
        self.test("fade-starts", |this| Box::pin(this.fade_starts()))
            .await?;
        self.test("fade-completes", |this| Box::pin(this.fade_completes()))
            .await?;
        self.test("same-scene-finish", |this| {
            Box::pin(this.same_scene_finish())
        })
        .await?;
        self.test("same-scene-override", |this| {
            Box::pin(this.same_scene_override())
        })
        .await?;
        self.test("decreasing-duration-final-targets", |this| {
            Box::pin(this.decreasing_duration_targets())
        })
        .await?;
        self.test("link-unlinked-scene", |this| Box::pin(this.link_unlinked()))
            .await?;
        self.test("lockout-blocks-recall", |this| Box::pin(this.lockout()))
            .await?;
        Ok(())
    }

    async fn test<F>(&mut self, name: &str, body: F) -> Result<(), String>
    where
        F: for<'a> FnOnce(
            &'a mut Self,
        )
            -> std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>,
    {
        let started = Instant::now();
        match body(self).await {
            Ok(()) => {
                self.report
                    .line(format!(
                        "TEST {name} PASS {}ms",
                        started.elapsed().as_millis()
                    ))
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            Err(error) => {
                self.report
                    .line(format!("TEST {name} FAIL {error}"))
                    .map_err(|write_error| write_error.to_string())?;
                Err(format!("{name}: {error}"))
            }
        }
    }

    async fn cue_list_create(&mut self) -> Result<(), String> {
        let result = self
            .commands
            .create_cue_list("Smoke Cue List".to_string())
            .await?;
        let id = result
            .cue_list
            .ok_or("create_cue_list returned no cue list")?
            .id;
        self.wait_state("projected smoke cue list", |state| {
            state.active_cue_list_id.as_deref() == Some(id.to_string().as_str())
                && state
                    .cue_lists
                    .iter()
                    .any(|list| list.id == id && list.name == "Smoke Cue List")
        })
        .await?;
        Ok(())
    }

    async fn connection(&mut self, timeout_ms: u64) -> Result<(), String> {
        self.commands
            .refresh_lv1_discovery(Some(timeout_ms))
            .await?;
        let state = self
            .wait_state("LV1 discovery", |state| {
                !state.discovered_lv1_systems.is_empty()
            })
            .await?;
        let identity = state.discovered_lv1_systems[0].identity.clone();
        self.commands.connect_lv1_system(identity.clone()).await?;
        self.wait_state("LV1 connection", |state| {
            state.connection == AppConnectionState::Connected
                && state.connected_lv1_identity.as_ref() == Some(&identity)
        })
        .await?;
        self.resolve_scenes().await
    }

    async fn startup_auto_connect(&mut self) -> Result<(), String> {
        self.commands.disconnect_lv1().await?;
        self.wait_state("LV1 disconnect", |state| {
            state.connection == AppConnectionState::Disconnected
        })
        .await?;
        self.commands.startup_auto_connect_lv1().await?;
        self.wait_state("startup auto-connect", |state| {
            state.connection == AppConnectionState::Connected
        })
        .await?;
        self.resolve_scenes().await
    }

    async fn new_session(&mut self) -> Result<(), String> {
        let result = self.commands.new_show_file().await?;
        let selected = result
            .selected_scene_internal_id
            .ok_or("new show selected no scene")?;
        self.resolve_scenes().await?;
        if self.scene_a.to_string() != selected {
            return Err("new show did not select Smoke A".to_string());
        }
        Ok(())
    }

    async fn empty_defaults(&mut self) -> Result<(), String> {
        self.new_session().await?;
        let scene_a = self.scene_a;
        let scene_b = self.scene_b;
        let state = self
            .wait_state("empty scene settings", move |state| {
                [scene_a, scene_b].iter().all(|id| {
                    state
                        .scene_configs
                        .iter()
                        .find(|scene| scene.internal_scene_id == *id)
                        .is_some_and(empty_scene)
                })
            })
            .await?;
        drop(state);
        Ok(())
    }

    async fn copy_paste(&mut self) -> Result<(), String> {
        self.new_session().await?;
        self.commands.store_scene_config(self.scene_a).await?;
        self.commands
            .set_scene_scope_faders_enabled(self.scene_a, true)
            .await?;
        self.commands
            .set_scene_scope_pan_enabled(self.scene_a, true)
            .await?;
        self.commands
            .set_channel_scoped(self.scene_a, 0, 1, true)
            .await?;
        self.commands
            .set_scene_duration_ms(self.scene_a, 1_234)
            .await?;
        self.commands.copy_scene_settings(self.scene_a).await?;
        self.commands.paste_scene_settings(self.scene_b).await?;
        let scene_a = self.scene_a;
        let scene_b = self.scene_b;
        self.wait_state("pasted scene settings", move |state| {
            let a = find_config(state, scene_a);
            let b = find_config(state, scene_b);
            matches!((a, b), (Some(a), Some(b)) if a.duration_ms == 1_234
                && b.duration_ms == a.duration_ms
                && b.scope_toggles == a.scope_toggles
                && b.channel_configs == a.channel_configs
                && b.scoped_channels == a.scoped_channels
                && state.show_file_dirty)
        })
        .await?;
        Ok(())
    }

    async fn clipboard_reset(&mut self) -> Result<(), String> {
        self.new_session().await?;
        self.wait_state("cleared scene settings clipboard", |state| {
            !state.scene_settings_clipboard_available
        })
        .await?;
        Ok(())
    }

    async fn scene_recall(&mut self) -> Result<(), String> {
        self.commands.recall_scene(self.scene_a).await?;
        self.wait_state("scene Smoke A", |state| {
            state
                .current_scene
                .as_ref()
                .is_some_and(|scene| scene.name == SMOKE_A)
        })
        .await?;
        Ok(())
    }

    async fn rapid_recall(&mut self) -> Result<(), String> {
        self.commands.set_scene_duration_ms(self.scene_a, 0).await?;
        self.commands.set_scene_duration_ms(self.scene_b, 0).await?;
        let a = self.commands.clone();
        let b = self.commands.clone();
        let scene_a = self.scene_a;
        let scene_b = self.scene_b;
        let (first, second) = tokio::join!(a.recall_scene(scene_a), async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            b.recall_scene(scene_b).await
        });
        first?;
        second?;
        self.wait_state("queued Smoke B recall", |state| {
            state
                .current_scene
                .as_ref()
                .is_some_and(|scene| scene.name == SMOKE_B)
        })
        .await?;
        Ok(())
    }

    async fn prepare_fades(&mut self) -> Result<(), String> {
        self.new_session().await?;
        self.raw_reset(0, TARGET_A).await?;
        self.commands.store_scene_config(self.scene_a).await?;
        self.raw_reset(1, TARGET_B).await?;
        self.commands.store_scene_config(self.scene_b).await?;
        for scene_id in [self.scene_a, self.scene_b] {
            self.commands
                .set_scene_scope_faders_enabled(scene_id, true)
                .await?;
            self.commands
                .set_channel_scoped(scene_id, GROUP, CHANNEL, true)
                .await?;
            self.commands.set_scene_duration_ms(scene_id, 1_000).await?;
        }
        Ok(())
    }

    async fn fade_starts(&mut self) -> Result<(), String> {
        self.prepare_fades().await?;
        self.reset(self.scene_a, TARGET_A).await?;
        self.commands.recall_scene(self.scene_b).await?;
        self.wait_for_gain("fade movement", TIMEOUT, |gain| gain > TARGET_A + 3.0)
            .await
    }

    async fn fade_completes(&mut self) -> Result<(), String> {
        self.reset(self.scene_a, TARGET_A).await?;
        self.commands.recall_scene(self.scene_b).await?;
        self.wait_gain(TARGET_B, TIMEOUT).await
    }

    async fn same_scene_finish(&mut self) -> Result<(), String> {
        self.set_same_scene_settings(true, 500).await?;
        let result = async {
            self.reset(self.scene_a, TARGET_A).await?;
            self.commands
                .set_scene_duration_ms(self.scene_b, SAME_SCENE_DURATION.as_millis() as u64)
                .await?;
            self.commands.recall_scene(self.scene_b).await?;
            self.wait_for_gain("same-scene fade movement", TIMEOUT, |gain| {
                (TARGET_A + 2.0..TARGET_B - GAIN_TOLERANCE).contains(&gain)
            })
            .await?;
            let repeated_at = Instant::now();
            self.commands.recall_scene(self.scene_b).await?;
            self.wait_gain(TARGET_B, Duration::from_secs(3)).await?;
            self.wait_state("same-scene projected fade completion", |state| {
                state.fade_state == advanced_show_control::projector::AppFadeState::Idle
            })
            .await?;
            if repeated_at.elapsed() >= SAME_SCENE_DURATION {
                return Err("same-scene recall restarted the full fade duration".to_string());
            }
            Ok(())
        }
        .await;
        let duration_cleanup = self
            .commands
            .set_scene_duration_ms(self.scene_b, 1_000)
            .await;
        let settings_cleanup = self.set_same_scene_settings(true, 500).await;
        result?;
        duration_cleanup?;
        settings_cleanup
    }

    async fn same_scene_override(&mut self) -> Result<(), String> {
        self.set_same_scene_settings(false, 500).await?;
        let result = async {
            self.reset(self.scene_a, TARGET_A).await?;
            self.commands
                .set_scene_duration_ms(self.scene_b, SAME_SCENE_DURATION.as_millis() as u64)
                .await?;
            self.commands.recall_scene(self.scene_b).await?;
            self.wait_for_gain("same-scene override movement", TIMEOUT, |gain| {
                (TARGET_A + 2.0..TARGET_B - GAIN_TOLERANCE).contains(&gain)
            })
            .await?;
            let repeated_at = Instant::now();
            self.commands.recall_scene(self.scene_b).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            if (self.gain().await? - TARGET_B).abs() <= GAIN_TOLERANCE {
                return Err("disabled same-scene finishing completed immediately".to_string());
            }
            self.wait_gain(TARGET_B, SAME_SCENE_DURATION + Duration::from_secs(5))
                .await?;
            if repeated_at.elapsed() < SAME_SCENE_DURATION {
                return Err("same-scene override did not use the full duration".to_string());
            }
            Ok(())
        }
        .await;
        let duration_cleanup = self
            .commands
            .set_scene_duration_ms(self.scene_b, 1_000)
            .await;
        let settings_cleanup = self.set_same_scene_settings(true, 500).await;
        result?;
        duration_cleanup?;
        settings_cleanup
    }

    async fn decreasing_duration_targets(&mut self) -> Result<(), String> {
        self.reset(self.scene_a, TARGET_A).await?;
        for (duration_ms, scene_id, target) in [
            (5_000, self.scene_b, TARGET_B),
            (3_000, self.scene_a, TARGET_A),
            (1_000, self.scene_b, TARGET_B),
            (500, self.scene_a, TARGET_A),
        ] {
            self.commands
                .set_scene_duration_ms(scene_id, duration_ms)
                .await?;
            self.commands.recall_scene(scene_id).await?;
            self.wait_gain(target, TIMEOUT).await?;
        }
        Ok(())
    }

    async fn set_same_scene_settings(
        &mut self,
        enabled: bool,
        threshold_ms: u64,
    ) -> Result<(), String> {
        let mut settings = self
            .projections
            .latest()
            .ok_or("projected settings unavailable")?
            .settings;
        settings.same_scene_recall_enabled = enabled;
        settings.same_scene_recall_threshold_ms = threshold_ms;
        self.commands.replace_app_settings(settings).await?;
        self.wait_state("projected same-scene settings", |state| {
            state.settings.same_scene_recall_enabled == enabled
                && state.settings.same_scene_recall_threshold_ms == threshold_ms
        })
        .await?;
        Ok(())
    }

    async fn reset(&self, scene_id: Uuid, target: f64) -> Result<(), String> {
        self.commands.recall_scene(scene_id).await?;
        self.debug_commands
            .set_channel_gain(GROUP, CHANNEL, target)
            .await?;
        self.wait_gain(target, TIMEOUT).await
    }

    async fn raw_reset(&self, scene_index: i32, target: f64) -> Result<(), String> {
        self.debug_commands.recall_lv1_scene(scene_index).await?;
        self.debug_commands
            .set_channel_gain(GROUP, CHANNEL, target)
            .await?;
        self.wait_gain(target, TIMEOUT).await
    }

    async fn gain(&self) -> Result<f64, String> {
        self.debug_commands.channel_gain(GROUP, CHANNEL).await
    }

    async fn wait_gain(&self, target: f64, timeout: Duration) -> Result<(), String> {
        self.wait_for_gain(&format!("gain {target}"), timeout, |gain| {
            (gain - target).abs() <= GAIN_TOLERANCE
        })
        .await
    }

    async fn wait_for_gain(
        &self,
        label: &str,
        timeout: Duration,
        predicate: impl Fn(f64) -> bool,
    ) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if predicate(self.gain().await?) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("timed out waiting for {label}"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn link_unlinked(&mut self) -> Result<(), String> {
        let id = Uuid::new_v4();
        let path = self.fixture_path("unlinked");
        write_fixture(
            &path,
            vec![ShowFileSceneConfig {
                internal_scene_id: Some(id),
                scene_index: Some(99),
                scene_name: "Debug Smoke Missing Scene".to_string(),
                duration_ms: 1_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: SceneScopeToggles::default(),
            }],
        )?;
        self.commands.open_show_file(Some(path)).await?;
        self.commands.link_scene_config(id, 0, true).await?;
        self.wait_state("linked unlinked scene", |state| {
            state.scene_configs.iter().any(|scene| {
                scene.internal_scene_id == id
                    && scene.scene_index == Some(0)
                    && scene.scene_name == SMOKE_A
            })
        })
        .await?;
        self.scene_a = id;
        Ok(())
    }

    async fn lockout(&mut self) -> Result<(), String> {
        self.commands.set_lockout(true).await?;
        let error = self
            .commands
            .recall_scene(self.scene_b)
            .await
            .expect_err("lockout recall unexpectedly succeeded");
        if !error.to_lowercase().contains("blocked") {
            return Err(format!("unexpected lockout error: {error}"));
        }
        self.commands.set_lockout(false).await?;
        Ok(())
    }

    async fn resolve_scenes(&mut self) -> Result<(), String> {
        let state = self
            .wait_state("Smoke A and Smoke B scene configs", |state| {
                find_named(state, 0, SMOKE_A).is_some() && find_named(state, 1, SMOKE_B).is_some()
            })
            .await?;
        self.scene_a = find_named(&state, 0, SMOKE_A).unwrap().internal_scene_id;
        self.scene_b = find_named(&state, 1, SMOKE_B).unwrap().internal_scene_id;
        Ok(())
    }

    async fn wait_state(
        &mut self,
        label: &str,
        predicate: impl Fn(&AppViewState) -> bool,
    ) -> Result<AppViewState, String> {
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        loop {
            if let Some(state) = self.projections.latest()
                && predicate(&state)
            {
                return Ok(state);
            }
            tokio::time::timeout_at(deadline, self.projections.changed())
                .await
                .map_err(|_| format!("timed out waiting for {label}"))?
                .map_err(|_| format!("projection closed while waiting for {label}"))?;
        }
    }

    fn fixture_path(&self, name: &str) -> PathBuf {
        self.root
            .join("logs")
            .join(format!("debug-smoke-{name}-{}.ascs", Uuid::new_v4()))
    }
}

fn find_named<'a>(state: &'a AppViewState, index: i32, name: &str) -> Option<&'a SceneConfig> {
    state
        .scene_configs
        .iter()
        .find(|scene| scene.scene_index == Some(index) && scene.scene_name == name)
}

fn find_config(state: &AppViewState, id: Uuid) -> Option<&SceneConfig> {
    state
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id == id)
}

fn empty_scene(scene: &SceneConfig) -> bool {
    scene.duration_ms == 0
        && !scene.scope_toggles.faders
        && !scene.scope_toggles.pan
        && scene.channel_configs.is_empty()
        && scene.scoped_channels.is_empty()
}

fn write_fixture(path: &Path, scene_configs: Vec<ShowFileSceneConfig>) -> Result<(), String> {
    let file = ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at: "1970-01-01T00:00:00.000Z".to_string(),
        safety: ShowFileSafety { lockout: false },
        scene_configs,
        cue_lists: Vec::new(),
        active_cue_list_id: None,
        cued_cue_entry_id: None,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?;
    std::fs::write(path, json).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_config_path_uses_debug_application_identity() {
        let production = app_config_dir();

        let smoke = smoke_config_dir();

        assert_eq!(
            smoke.file_name().and_then(|name| name.to_str()),
            Some("com.advancedshowcontrol.debug")
        );
        assert_eq!(smoke.parent(), production.parent());
        assert_ne!(smoke, production);
    }

    #[test]
    fn exact_smoke_scene_resolution_rejects_name_and_index_lookalikes() {
        let id = Uuid::new_v4();
        let mut state = AppViewState::default();
        state.scene_configs.push(SceneConfig {
            internal_scene_id: id,
            scene_index: Some(0),
            scene_name: "Smoke A renamed".to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        });
        assert!(find_named(&state, 0, SMOKE_A).is_none());
        state.scene_configs[0].scene_name = SMOKE_A.to_string();
        assert_eq!(
            find_named(&state, 0, SMOKE_A).unwrap().internal_scene_id,
            id
        );
        assert!(find_named(&state, 1, SMOKE_A).is_none());
    }

    #[test]
    fn shutdown_failure_prevents_suite_pass() {
        let root = std::env::temp_dir().join(format!("asc-smoke-shutdown-test-{}", Uuid::new_v4()));
        let mut report = SmokeReport::create(&root).unwrap();
        let mut suite = SuiteResult::default();

        assert!(!finish_report(&mut report, &mut suite, false));
        let contents = std::fs::read_to_string(report.path()).unwrap();
        assert!(contents.contains("ERROR runtime shutdown cleanup failed"));
        assert!(contents.ends_with("SUITE FAIL\n"));
        assert!(!contents.contains("SUITE PASS"));

        drop(report);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_scene_check_covers_all_settings() {
        let mut scene = SceneConfig {
            internal_scene_id: Uuid::new_v4(),
            scene_index: Some(0),
            scene_name: SMOKE_A.to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        };
        assert!(empty_scene(&scene));
        scene.duration_ms = 1;
        assert!(!empty_scene(&scene));
    }
}

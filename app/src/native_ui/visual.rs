#[cfg(target_os = "macos")]
mod macos {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::{Context as _, Result};
    use gpui_kit::component::{Root, WindowExt as _};
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, HeadlessAppContext, px, size};
    use uuid::Uuid;

    use crate::connection_state::{DiscoveredLv1Status, DiscoveredLv1System, Lv1SystemIdentity};
    use crate::cue_lists::{CueEntry, CueList};
    use crate::projector::{
        AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
        SceneSummary,
    };
    use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles};

    use super::super::cues::CueListsView;
    use super::super::scenes::ScenesView;
    use super::super::settings_view::SettingsView;
    use super::super::{
        AppRoot, CommandDispatcher, NativeRuntime, UiEvent, theme, ui_event_channel,
    };

    macro_rules! visual_signature {
        ($image:expr) => {{
            const COLUMNS: u32 = 59;
            const ROWS: u32 = 39;
            let image = &$image;
            let (width, height) = image.dimensions();
            let mut signature = Vec::with_capacity((COLUMNS * ROWS * 3) as usize);
            for row in 0..ROWS {
                for column in 0..COLUMNS {
                    if row >= 36 && column >= 47 {
                        signature.extend_from_slice(&[0, 0, 0]);
                        continue;
                    }
                    let x_start = column * width / COLUMNS;
                    let x_end = (column + 1) * width / COLUMNS;
                    let y_start = row * height / ROWS;
                    let y_end = (row + 1) * height / ROWS;
                    let mut totals = [0_u64; 3];
                    let mut count = 0_u64;
                    for y in y_start..y_end {
                        for x in x_start..x_end {
                            let pixel = image.get_pixel(x, y).0;
                            for channel in 0..3 {
                                totals[channel] += u64::from(pixel[channel]);
                            }
                            count += 1;
                        }
                    }
                    for total in totals {
                        signature.push((total / count.max(1)) as u8);
                    }
                }
            }
            signature
        }};
    }

    pub fn run(output_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(output_dir).with_context(|| {
            format!(
                "failed to create native visual output directory {}",
                output_dir.display()
            )
        })?;
        let config_dir = output_dir.join("runtime-config");
        let mut runtime = NativeRuntime::build_in(config_dir)?;
        let projections = runtime.take_projections();
        let (ui_events, receiver) = ui_event_channel();
        let dispatcher =
            CommandDispatcher::new(runtime.handle(), runtime.commands(), ui_events.clone());

        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            gpui_kit::init(cx);
            theme::install(cx).expect("bundled native theme must install");
        });

        let window = cx
            .open_window(size(px(1180.), px(780.)), |window, cx| {
                let initial = AppViewState::default();
                let scenes =
                    cx.new(|cx| ScenesView::new(initial.clone(), dispatcher.clone(), window, cx));
                let cues =
                    cx.new(|cx| CueListsView::new(initial.clone(), dispatcher.clone(), window, cx));
                let settings =
                    cx.new(|cx| SettingsView::new(initial, dispatcher.clone(), window, cx));
                let app = cx.new(|cx| {
                    AppRoot::new(
                        dispatcher,
                        projections,
                        receiver,
                        scenes,
                        cues,
                        settings,
                        window,
                        cx,
                    )
                });
                cx.new(|cx| Root::new(app, window, cx))
            })
            .context("failed to open native visual test window")?;

        let mut offline = reference_snapshot(u64::MAX - 5, false);
        offline.connection = AppConnectionState::Disconnected;
        offline.connected_lv1_identity = None;
        ui_events
            .send(UiEvent::Snapshot(Box::new(offline)))
            .map_err(|_| anyhow::anyhow!("native visual event receiver closed"))?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))?;
        let connection = cx.capture_screenshot(window.into())?;
        connection
            .save(output_dir.join("native-connection.png"))
            .context("failed to save connection screenshot")?;

        ui_events
            .send(UiEvent::Snapshot(Box::new(reference_snapshot(
                u64::MAX - 4,
                false,
            ))))
            .map_err(|_| anyhow::anyhow!("native visual event receiver closed"))?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            if window.has_active_dialog(cx) {
                window.close_all_dialogs(cx);
            }
        })?;
        cx.advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))?;
        let ready = cx.capture_screenshot(window.into())?;
        ready
            .save(output_dir.join("native-shell-ready.png"))
            .context("failed to save ready-state screenshot")?;

        let mut tab_captures = Vec::new();
        let mut cue_manager_capture = None;
        for (selector, file_name) in [
            ("tab-Cue Lists", "native-cue-lists.png"),
            ("tab-Settings", "native-settings.png"),
            ("tab-Logs", "native-logs.png"),
        ] {
            cx.update_window(window.into(), |_, window, cx| {
                window.click(selector, cx);
                window.render_frame(cx);
            })?;
            let image = cx.capture_screenshot(window.into())?;
            image
                .save(output_dir.join(file_name))
                .with_context(|| format!("failed to save {file_name}"))?;
            tab_captures.push(image);
            if selector == "tab-Settings" {
                cx.update_window(window.into(), |_, window, cx| {
                    window.click("go-shortcut", cx);
                    window.press("cmd-s", cx);
                })?;
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    assert!(window.try_find("go-shortcut-conflict").is_some());
                    assert!(!window.has_active_prompt());
                })?;
            }
            if selector == "tab-Cue Lists" {
                cx.update_window(window.into(), |_, window, cx| {
                    window.click("manage-cue-lists", cx);
                })?;
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                })?;
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    assert!(gpui_kit::base::active_focus_trap(window, cx).is_some());
                })?;
                let manager = cx.capture_screenshot(window.into())?;
                manager
                    .save(output_dir.join("native-cue-manager.png"))
                    .context("failed to save cue-manager screenshot")?;
                cue_manager_capture = Some(manager);
                cx.update_window(window.into(), |_, window, cx| {
                    window.click("delete-cue-list-33333333-3333-4333-8333-333333333333", cx);
                    window.render_frame(cx);
                    assert!(window.try_find("delete-cue-list-confirmation").is_some());
                    assert!(gpui_kit::base::active_focus_trap(window, cx).is_some());
                    window.press("escape", cx);
                })?;
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    assert!(window.try_find("delete-cue-list-confirmation").is_none());
                    assert!(gpui_kit::base::active_focus_trap(window, cx).is_some());
                    window.press("escape", cx);
                })?;
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    assert!(gpui_kit::base::active_focus_trap(window, cx).is_none());
                })?;
            }
        }
        cx.update_window(window.into(), |_, window, cx| {
            window.click("tab-Scenes", cx);
            window.render_frame(cx);
        })?;

        ui_events
            .send(UiEvent::Snapshot(Box::new(reference_snapshot(
                u64::MAX - 3,
                true,
            ))))
            .map_err(|_| anyhow::anyhow!("native visual event receiver closed"))?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.render_frame(cx))?;
        let safe = cx.capture_screenshot(window.into())?;
        safe.save(output_dir.join("native-shell-safe.png"))
            .context("failed to save safe-state screenshot")?;

        let unlinked_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
        let mut overwrite_state = reference_snapshot(u64::MAX - 2, false);
        let mut unlinked = scene_config(unlinked_id, 0, "Imported Fade", 2_000);
        unlinked.scene_index = None;
        overwrite_state.scene_configs.push(unlinked);
        overwrite_state.selected_scene_internal_id = Some(unlinked_id.to_string());
        ui_events
            .send(UiEvent::Snapshot(Box::new(overwrite_state)))
            .map_err(|_| anyhow::anyhow!("native visual event receiver closed"))?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("link-scene", cx);
        })?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
        })?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(gpui_kit::base::active_focus_trap(window, cx).is_some());
        })?;
        let scene_overwrite = cx.capture_screenshot(window.into())?;
        scene_overwrite
            .save(output_dir.join("native-scene-overwrite.png"))
            .context("failed to save scene-overwrite screenshot")?;
        cx.update_window(window.into(), |_, window, cx| {
            window.press("escape", cx);
        })?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
        })?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(gpui_kit::base::active_focus_trap(window, cx).is_none());
        })?;

        let dimensions = ready.dimensions();
        for image in std::iter::once(&connection)
            .chain(tab_captures.iter())
            .chain(cue_manager_capture.iter())
            .chain([&safe, &scene_overwrite])
        {
            anyhow::ensure!(dimensions == image.dimensions());
        }
        anyhow::ensure!(dimensions.0 >= 1180 && dimensions.1 >= 780);
        anyhow::ensure!(
            dimensions.0 * 780 == dimensions.1 * 1180,
            "native screenshot has the wrong aspect ratio: {dimensions:?}"
        );
        anyhow::ensure!(
            ready != safe,
            "distinct projected states rendered identically"
        );
        anyhow::ensure!(
            tab_captures.iter().all(|image| image != &ready),
            "tab interaction did not change the rendered native view"
        );

        let visual_snapshots = [
            (
                "native-connection",
                visual_signature!(connection),
                include_bytes!("visual_snapshots/native-connection.rgb").as_slice(),
            ),
            (
                "native-shell-ready",
                visual_signature!(ready),
                include_bytes!("visual_snapshots/native-shell-ready.rgb").as_slice(),
            ),
            (
                "native-cue-lists",
                visual_signature!(tab_captures[0]),
                include_bytes!("visual_snapshots/native-cue-lists.rgb").as_slice(),
            ),
            (
                "native-cue-manager",
                visual_signature!(cue_manager_capture.as_ref().unwrap()),
                include_bytes!("visual_snapshots/native-cue-manager.rgb").as_slice(),
            ),
            (
                "native-settings",
                visual_signature!(tab_captures[1]),
                include_bytes!("visual_snapshots/native-settings.rgb").as_slice(),
            ),
            (
                "native-logs",
                visual_signature!(tab_captures[2]),
                include_bytes!("visual_snapshots/native-logs.rgb").as_slice(),
            ),
            (
                "native-shell-safe",
                visual_signature!(safe),
                include_bytes!("visual_snapshots/native-shell-safe.rgb").as_slice(),
            ),
            (
                "native-scene-overwrite",
                visual_signature!(scene_overwrite),
                include_bytes!("visual_snapshots/native-scene-overwrite.rgb").as_slice(),
            ),
        ];
        let update_snapshots = std::env::var_os("ASC_UPDATE_NATIVE_VISUALS").is_some();
        for (name, actual, expected) in visual_snapshots {
            if update_snapshots {
                std::fs::write(visual_snapshot_path(name), actual)
                    .with_context(|| format!("failed to update native visual snapshot {name}"))?;
            } else {
                compare_visual_signature(name, &actual, expected)?;
            }
        }

        drop(cx);
        runtime.shutdown();
        println!("native visual checks passed: {}", output_dir.display());
        Ok(())
    }

    fn visual_snapshot_path(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/native_ui/visual_snapshots")
            .join(format!("{name}.rgb"))
    }

    fn compare_visual_signature(name: &str, actual: &[u8], expected: &[u8]) -> Result<()> {
        anyhow::ensure!(
            actual.len() == expected.len(),
            "native visual snapshot {name} has not been initialized; run ASC_UPDATE_NATIVE_VISUALS=1 make visual-test"
        );
        let mut total_difference = 0_u64;
        let mut materially_changed = 0_usize;
        for (&actual, &expected) in actual.iter().zip(expected) {
            let difference = actual.abs_diff(expected);
            total_difference += u64::from(difference);
            materially_changed += usize::from(difference > 24);
        }
        let mean_difference = total_difference as f64 / actual.len() as f64;
        let materially_changed_ratio = materially_changed as f64 / actual.len() as f64;
        anyhow::ensure!(
            mean_difference <= 6.0 && materially_changed_ratio <= 0.10,
            "native visual snapshot {name} changed (mean channel difference {mean_difference:.2}, material ratio {materially_changed_ratio:.3}); inspect the rendered PNG and update intentionally with ASC_UPDATE_NATIVE_VISUALS=1 make visual-test"
        );
        Ok(())
    }

    fn reference_snapshot(state_version: u64, lockout: bool) -> AppViewState {
        let scene_a = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let scene_b = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let list_id = Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
        let entry_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let scenes = vec![
            SceneSummary {
                index: 0,
                name: "Opening".into(),
            },
            SceneSummary {
                index: 1,
                name: "Walk In".into(),
            },
        ];
        let scene_configs = vec![
            scene_config(scene_a, 0, "Opening", 1_500),
            scene_config(scene_b, 1, "Walk In", 3_000),
        ];
        let connected_identity = Lv1SystemIdentity {
            uuid: Some("visual-console".into()),
            host: Some("FOH Console".into()),
            address: "192.0.2.10".into(),
            port: 10_011,
        };
        AppViewState {
            connection: AppConnectionState::Connected,
            connected_lv1_identity: Some(connected_identity.clone()),
            discovered_lv1_systems: vec![
                DiscoveredLv1System {
                    identity: connected_identity,
                    status: DiscoveredLv1Status::Available,
                },
                DiscoveredLv1System {
                    identity: Lv1SystemIdentity {
                        uuid: Some("visual-console-b".into()),
                        host: Some("Broadcast Console".into()),
                        address: "192.0.2.20".into(),
                        port: 10_011,
                    },
                    status: DiscoveredLv1Status::Unavailable,
                },
            ],
            current_scene: Some(scenes[0].clone()),
            scenes,
            scene_count: 2,
            channel_count: 2,
            channels: vec![
                ChannelSummary {
                    group: 0,
                    channel: 1,
                    name: "Lead Vocal".into(),
                },
                ChannelSummary {
                    group: 0,
                    channel: 2,
                    name: "Guitar".into(),
                },
            ],
            fade_state: if lockout {
                AppFadeState::Blocked
            } else {
                AppFadeState::Idle
            },
            lockout,
            scene_configs,
            cue_lists: vec![CueList {
                id: list_id,
                name: "Main Show".into(),
                entries: vec![CueEntry {
                    id: entry_id,
                    scene_internal_id: scene_b,
                }],
            }],
            active_cue_list_id: Some(list_id.to_string()),
            cued_cue_entry_id: Some(entry_id.to_string()),
            selected_scene_internal_id: Some(scene_a.to_string()),
            show_file_name: "Visual Reference.ascs".into(),
            show_file_dirty: lockout,
            logs: vec![AppLogEntry {
                id: 1,
                timestamp: "12:34:56".into(),
                severity: LogSeverity::Info,
                message: "Connected to FOH Console".into(),
            }],
            state_version,
            ..Default::default()
        }
    }

    fn scene_config(id: Uuid, index: i32, name: &str, duration_ms: u64) -> SceneConfig {
        SceneConfig {
            internal_scene_id: id,
            scene_index: Some(index),
            scene_name: name.into(),
            duration_ms,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 1,
                fader_db: Some(-6.0),
                pan: Some(0.0),
                balance: None,
                width: None,
                pan_mode: Some(crate::lv1::PanMode::Mono),
            }],
            scoped_channels: vec![ChannelRef {
                group: 0,
                channel: 1,
            }],
            scope_toggles: SceneScopeToggles {
                faders: true,
                pan: false,
            },
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::run;

#[cfg(not(target_os = "macos"))]
pub fn run(_: &std::path::Path) -> anyhow::Result<()> {
    anyhow::bail!("native screenshot checks require GPUI Kit's macOS headless renderer")
}

pub fn run_gallery() -> anyhow::Result<()> {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui_kit::component::Root;
    use gpui_kit::{AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

    use super::cues::CueListsView;
    use super::scenes::ScenesView;
    use super::settings_view::SettingsView;
    use super::{AppRoot, CommandDispatcher, NativeRuntime, UiEvent, theme, ui_event_channel};

    let config_dir = std::env::current_dir()?.join("target/native-gallery-config");
    let mut runtime = NativeRuntime::build_in(config_dir)?;
    let projections = runtime.take_projections();
    let (ui_events, receiver) = ui_event_channel();
    let dispatcher =
        CommandDispatcher::new(runtime.handle(), runtime.commands(), ui_events.clone());
    let runtime = Rc::new(RefCell::new(Some(runtime)));
    let shutdown_runtime = runtime.clone();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            theme::install(cx).expect("bundled native theme must install");
            super::menu::install(cx);
            cx.on_app_quit(move |cx| {
                let shutdown = shutdown_runtime.borrow_mut().take().map(|runtime| {
                    cx.background_executor()
                        .spawn(async move { runtime.shutdown() })
                });
                async move {
                    if let Some(shutdown) = shutdown {
                        let _ = shutdown.await;
                    }
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
            let gallery_events = ui_events.clone();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(960.), px(640.))),
                    app_id: Some(super::APP_IDENTIFIER.to_string()),
                    ..Default::default()
                },
                move |window, cx| {
                    let initial = crate::projector::AppViewState::default();
                    let scenes = cx
                        .new(|cx| ScenesView::new(initial.clone(), dispatcher.clone(), window, cx));
                    let cues = cx.new(|cx| {
                        CueListsView::new(initial.clone(), dispatcher.clone(), window, cx)
                    });
                    let settings =
                        cx.new(|cx| SettingsView::new(initial, dispatcher.clone(), window, cx));
                    let app = cx.new(|cx| {
                        AppRoot::new(
                            dispatcher,
                            projections,
                            receiver,
                            scenes,
                            cues,
                            settings,
                            window,
                            cx,
                        )
                    });
                    let gallery = cx.new(|_| GalleryRoot {
                        app,
                        ui_events: gallery_events.clone(),
                        state_version: GALLERY_STATE_VERSION,
                        safe: false,
                    });
                    cx.new(|cx| Root::new(gallery, window, cx))
                },
            )
            .expect("failed to open native gallery window");
            ui_events
                .send(UiEvent::Snapshot(Box::new(gallery_snapshot(
                    GALLERY_STATE_VERSION,
                    false,
                ))))
                .expect("native gallery event receiver closed");
            cx.activate(true);
        });

    if let Some(runtime) = runtime.borrow_mut().take() {
        runtime.shutdown();
    }
    Ok(())
}

const GALLERY_STATE_VERSION: u64 = u64::MAX - 100;

struct GalleryRoot {
    app: gpui_kit::Entity<super::AppRoot>,
    ui_events: tokio::sync::mpsc::UnboundedSender<super::UiEvent>,
    state_version: u64,
    safe: bool,
}

impl GalleryRoot {
    fn show_state(&mut self, safe: bool, cx: &mut gpui_kit::Context<Self>) {
        if self.safe == safe {
            return;
        }
        self.safe = safe;
        self.state_version += 1;
        self.ui_events
            .send(super::UiEvent::Snapshot(Box::new(gallery_snapshot(
                self.state_version,
                safe,
            ))))
            .expect("native gallery event receiver closed");
        cx.notify();
    }
}

impl gpui_kit::Render for GalleryRoot {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::component::Disableable as _;
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        use gpui_kit::{ParentElement as _, Styled as _, div, px, rgb};

        div().relative().size_full().child(self.app.clone()).child(
            div()
                .absolute()
                .top(px(88.))
                .right(px(18.))
                .flex()
                .gap_2()
                .p_2()
                .border_1()
                .border_color(rgb(super::theme::CONSOLE_LINE))
                .bg(rgb(super::theme::CONSOLE_CHROME))
                .child(
                    Button::new("gallery-ready-state")
                        .primary()
                        .label("READY STATE")
                        .disabled(!self.safe)
                        .on_click(cx.listener(|this, _, _, cx| this.show_state(false, cx))),
                )
                .child(
                    Button::new("gallery-safe-state")
                        .warning()
                        .label("SAFE STATE")
                        .disabled(self.safe)
                        .on_click(cx.listener(|this, _, _, cx| this.show_state(true, cx))),
                ),
        )
    }
}

fn gallery_snapshot(state_version: u64, lockout: bool) -> crate::projector::AppViewState {
    use crate::connection_state::{DiscoveredLv1Status, DiscoveredLv1System, Lv1SystemIdentity};
    use crate::cue_lists::{CueEntry, CueList};
    use crate::projector::{
        AppConnectionState, AppFadeState, AppLogEntry, ChannelSummary, LogSeverity, SceneSummary,
    };
    use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles};

    let scene_a = uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let scene_b = uuid::Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
    let list_id = uuid::Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let entry_id = uuid::Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
    let scenes = vec![
        SceneSummary {
            index: 0,
            name: "Opening".into(),
        },
        SceneSummary {
            index: 1,
            name: "Walk In".into(),
        },
    ];
    let connected_identity = Lv1SystemIdentity {
        uuid: Some("gallery-console".into()),
        host: Some("FOH Console".into()),
        address: "192.0.2.10".into(),
        port: 10_011,
    };
    let scene_config = |id: uuid::Uuid, index: i32, name: &str, duration_ms: u64| SceneConfig {
        internal_scene_id: id,
        scene_index: Some(index),
        scene_name: name.into(),
        duration_ms,
        channel_configs: vec![ChannelConfig {
            group: 0,
            channel: 1,
            fader_db: Some(-6.0),
            pan: Some(0.0),
            balance: None,
            width: None,
            pan_mode: Some(crate::lv1::PanMode::Mono),
        }],
        scoped_channels: vec![ChannelRef {
            group: 0,
            channel: 1,
        }],
        scope_toggles: SceneScopeToggles {
            faders: true,
            pan: false,
        },
    };

    crate::projector::AppViewState {
        connection: AppConnectionState::Connected,
        connected_lv1_identity: Some(connected_identity.clone()),
        discovered_lv1_systems: vec![DiscoveredLv1System {
            identity: connected_identity,
            status: DiscoveredLv1Status::Available,
        }],
        current_scene: Some(scenes[0].clone()),
        scenes,
        scene_count: 2,
        channel_count: 2,
        channels: vec![
            ChannelSummary {
                group: 0,
                channel: 1,
                name: "Lead Vocal".into(),
            },
            ChannelSummary {
                group: 0,
                channel: 2,
                name: "Guitar".into(),
            },
        ],
        fade_state: if lockout {
            AppFadeState::Blocked
        } else {
            AppFadeState::Idle
        },
        lockout,
        scene_configs: vec![
            scene_config(scene_a, 0, "Opening", 1_500),
            scene_config(scene_b, 1, "Walk In", 3_000),
        ],
        cue_lists: vec![CueList {
            id: list_id,
            name: "Main Show".into(),
            entries: vec![CueEntry {
                id: entry_id,
                scene_internal_id: scene_b,
            }],
        }],
        active_cue_list_id: Some(list_id.to_string()),
        cued_cue_entry_id: Some(entry_id.to_string()),
        selected_scene_internal_id: Some(scene_a.to_string()),
        show_file_name: "Component Gallery.ascs".into(),
        show_file_dirty: lockout,
        logs: vec![AppLogEntry {
            id: 1,
            timestamp: "12:34:56".into(),
            severity: LogSeverity::Info,
            message: "Connected to FOH Console".into(),
        }],
        state_version,
        ..Default::default()
    }
}

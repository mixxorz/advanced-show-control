#[cfg(target_os = "macos")]
mod macos {
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use anyhow::{Context as _, Result};
    use gpui_kit::component::{Root, WindowExt as _};
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, HeadlessAppContext, ScrollDelta, point, px, size};
    use uuid::Uuid;

    use crate::connection_state::{DiscoveredLv1Status, DiscoveredLv1System, Lv1SystemIdentity};
    use crate::cue_lists::{CueEntry, CueList};
    use crate::projector::{
        AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
        SceneSummary,
    };
    use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles};

    use super::super::cues::CueListsView;
    use super::super::menu::{self, MENU_NEW_SHORTCUT, NewShow, Quit};
    use super::super::scenes::ScenesView;
    use super::super::settings_view::SettingsView;
    use super::super::state::GoSubmissionGuard;
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
        let latency_probes = Arc::new(Mutex::new(Vec::new()));
        let captured_latency_probes = latency_probes.clone();
        let dispatcher =
            CommandDispatcher::new(runtime.handle(), runtime.commands(), ui_events.clone())
                .with_latency_probe_override(move |session, attempt, identity, timeout| {
                    captured_latency_probes
                        .lock()
                        .unwrap()
                        .push((session, attempt, identity, timeout));
                });
        let observed_dispatcher = dispatcher.clone();

        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            gpui_kit::init(cx);
            theme::install(cx).expect("bundled native theme must install");
            menu::install(cx);
        });

        let window = cx
            .open_window(size(px(1180.), px(780.)), |window, cx| {
                let initial = AppViewState::default();
                let go_submissions = Rc::new(RefCell::new(GoSubmissionGuard::default()));
                go_submissions.borrow_mut().start(
                    u64::MAX,
                    0,
                    0,
                    Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
                );
                let scenes =
                    cx.new(|cx| ScenesView::new(initial.clone(), dispatcher.clone(), window, cx));
                let cues = cx.new(|cx| {
                    CueListsView::new(
                        initial.clone(),
                        dispatcher.clone(),
                        go_submissions.clone(),
                        window,
                        cx,
                    )
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
                        go_submissions,
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
        let measured_identity = offline.discovered_lv1_systems[0].identity.clone();
        ui_events
            .send(UiEvent::Snapshot(Box::new(offline)))
            .map_err(|_| anyhow::anyhow!("native visual event receiver closed"))?;
        cx.run_until_parked();
        let (session_id, attempt_id, probed_identity, timeout) = latency_probes
            .lock()
            .unwrap()
            .iter()
            .find(|(_, _, identity, _)| identity == &measured_identity)
            .cloned()
            .context("automatic latency probe was not dispatched")?;
        anyhow::ensure!(
            timeout.is_none(),
            "visual latency probe changed its timeout"
        );
        ui_events
            .send(UiEvent::LatencyMeasured {
                session_id,
                attempt_id,
                identity: probed_identity,
                result: Ok(crate::lv1::TcpConnectProbeResult { tcp_connect_ms: 12 }),
            })
            .map_err(|_| anyhow::anyhow!("native visual event receiver closed"))?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(
                window
                    .find(format!(
                        "select-system-{}",
                        super::super::connection::identity_key(&measured_identity)
                    ))
                    .label(),
                Some("FOH Console, Available, latency 12 ms")
            );
        })?;
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
        let dispatched_before_new_shortcut = observed_dispatcher.dispatched_count();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.is_action_available(&NewShow, cx));
            assert!(window.is_action_available(&Quit, cx));
            window.press(MENU_NEW_SHORTCUT, cx);
        })?;
        anyhow::ensure!(
            observed_dispatcher.dispatched_count() == dispatched_before_new_shortcut + 1,
            "New Session shortcut did not dispatch immediately after the startup dialog closed"
        );

        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            let top_bar = window.find("top-bar").bounds();
            let session_menu = window.find("session-menu").bounds();
            let scenes_tab = window.find("tab-Scenes").bounds();
            let cue_lists_tab = window.find("tab-Cue Lists").bounds();
            let logs_tab = window.find("tab-Logs").bounds();
            assert_eq!(session_menu.size.width, session_menu.size.height);
            assert_eq!(session_menu.top(), top_bar.top() + px(1.));
            assert_eq!(session_menu.bottom(), top_bar.bottom() - px(1.));
            assert_eq!(session_menu.right(), scenes_tab.left());
            assert!(window.try_find("tab-Events").is_none());
            assert_eq!(cue_lists_tab.right(), logs_tab.left());

            let connection_dot = window.find("connection-status-dot").bounds();
            let connection_label = window.find("connection-status-label").bounds();
            assert_eq!(connection_dot.size, size(px(8.), px(8.)));
            assert_eq!(connection_dot.center().y, connection_label.center().y);

            let console_chooser = window.find("open-connection").bounds();
            assert!(console_chooser.size.width >= px(144.));
            assert!(window.try_find("scene-x-fade").is_some());

            let bottom_status = window.find("bottom-status").bounds();
            let go_cell = window.find("go-cell").bounds();
            let go = window.find("go").bounds();
            assert!(go_cell.size.width >= bottom_status.size.width * 0.13);
            assert!(go_cell.size.width <= bottom_status.size.width * 0.15);
            assert!(go.size.width >= go_cell.size.width * 0.80);
            assert!(go.size.height >= go_cell.size.height * 0.75);
            let status_cells = [
                window.find("status-next").bounds(),
                window.find("status-current").bounds(),
                window.find("status-mode").bounds(),
                window.find("status-time").bounds(),
            ];
            for cell in status_cells.iter().skip(1) {
                assert!((cell.size.width - status_cells[0].size.width).abs() <= px(1.));
            }

            let probes_before_open = latency_probes.lock().unwrap().len();
            window.click("open-connection", cx);
            window.render_frame(cx);
            let probes_after_open = latency_probes.lock().unwrap();
            assert_eq!(probes_after_open.len(), probes_before_open + 2);
            let current_probe = probes_after_open
                .iter()
                .rev()
                .find(|(_, _, identity, _)| identity == &measured_identity)
                .unwrap();
            assert_ne!((current_probe.0, current_probe.1), (session_id, attempt_id));
            drop(probes_after_open);
            let overlay = window.find("connection-focus-trap").bounds();
            let modal = window.find("connection-modal").bounds();
            assert_eq!(overlay.origin, point(px(0.), px(0.)));
            assert_eq!(overlay.size, size(px(1180.), px(780.)));
            assert!((modal.center().x - overlay.center().x).abs() <= px(0.5));
            assert!((modal.center().y - overlay.center().y).abs() <= px(0.5));
            assert!(modal.size.width < overlay.size.width * 0.7);
            assert_eq!(
                window.find("connection-focus-trap").label(),
                Some("Connect to LV1")
            );
            assert!(gpui_kit::base::active_focus_trap(window, cx).is_some());
            assert!(window.is_action_available(&Quit, cx));
        })?;
        let connected_connection = cx.capture_screenshot(window.into())?;
        connected_connection
            .save(output_dir.join("native-connection-connected.png"))
            .context("failed to save connected connection screenshot")?;
        let dispatched_while_modal = observed_dispatcher.dispatched_count();
        cx.update_window(window.into(), |_, window, cx| {
            window.press(MENU_NEW_SHORTCUT, cx);
            window.press("space", cx);
            assert_eq!(
                observed_dispatcher.dispatched_count(),
                dispatched_while_modal
            );
            window.press("escape", cx);
        })?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("connection-focus-trap").is_none());
            assert!(gpui_kit::base::active_focus_trap(window, cx).is_none());
            assert!(window.focused(cx).is_some());
        })?;
        let ready = cx.capture_screenshot(window.into())?;
        ready
            .save(output_dir.join("native-shell-ready.png"))
            .context("failed to save ready-state screenshot")?;

        cx.update_window(window.into(), |_, window, cx| {
            window.click("session-menu", cx);
            window.render_frame(cx);
            let frame = window.find("session-menu-frame").bounds();
            let popup = window.find("popup-menu").bounds();
            assert_eq!(frame.top() + px(1.), popup.top());
            assert_eq!(frame.left() + px(1.), popup.left());
            assert_eq!(frame.bottom() - px(1.), popup.bottom());
            assert_eq!(frame.right() - px(1.), popup.right());
        })?;
        let session_menu = cx.capture_screenshot(window.into())?;
        session_menu
            .save(output_dir.join("native-session-menu.png"))
            .context("failed to save session-menu screenshot")?;
        cx.update_window(window.into(), |_, window, cx| {
            window.press("escape", cx);
        })?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("popup-menu").is_none());
            window.click("tab-Cue Lists", cx);
            window.render_frame(cx);
            assert!(
                window
                    .try_find("cue-active-scene-44444444-4444-4444-8444-444444444444")
                    .is_some()
            );
            assert!(
                window
                    .try_find("cue-active-scene-66666666-6666-4666-8666-666666666666")
                    .is_some()
            );
            assert!(
                window
                    .try_find("cue-active-scene-77777777-7777-4777-8777-777777777777")
                    .is_some()
            );
            assert!(
                window
                    .try_find("cue-active-scene-55555555-5555-4555-8555-555555555555")
                    .is_none()
            );
            window.click("select-cue-entry-44444444-4444-4444-8444-444444444444", cx);
            window.click("session-menu", cx);
            window.render_frame(cx);
            assert!(window.try_find("popup-menu").is_some());
        })?;
        let dispatched_before_shortcuts = observed_dispatcher.dispatched_count();
        cx.update_window(window.into(), |_, window, cx| {
            window.press("space", cx);
            window.press("c", cx);
        })?;
        cx.run_until_parked();
        anyhow::ensure!(
            observed_dispatcher.dispatched_count() == dispatched_before_shortcuts,
            "GO or Cue dispatched while the session menu was open"
        );
        cx.update_window(window.into(), |_, window, cx| {
            window.press("escape", cx);
            window.click("select-cue-entry-77777777-7777-4777-8777-777777777777", cx);
        })?;
        let dispatched_before_keyboard_go = observed_dispatcher.dispatched_count();
        cx.update_window(window.into(), |_, window, cx| {
            window.press("space", cx);
            assert!(window.focused(cx).is_some());
        })?;
        cx.run_until_parked();
        anyhow::ensure!(
            observed_dispatcher.dispatched_count() == dispatched_before_keyboard_go + 1,
            "keyboard GO did not dispatch exactly once"
        );
        let dispatched_after_keyboard_go = observed_dispatcher.dispatched_count();
        cx.update_window(window.into(), |_, window, cx| {
            window.click("cue-selected", cx);
        })?;
        cx.run_until_parked();
        anyhow::ensure!(
            observed_dispatcher.dispatched_count() == dispatched_after_keyboard_go,
            "keyboard GO left the focused cue row selected"
        );
        let dispatched_before_pointer_go = observed_dispatcher.dispatched_count();
        cx.update_window(window.into(), |_, window, cx| {
            window.click("go", cx);
        })?;
        cx.run_until_parked();
        anyhow::ensure!(
            observed_dispatcher.dispatched_count() == dispatched_before_pointer_go + 1,
            "pointer GO did not use the shared submission path"
        );

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
            if selector == "tab-Logs" {
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    let timestamp = window.find("log-timestamp-0").bounds();
                    let severity = window.find("log-severity-0").bounds();
                    let message = window.find("log-message-0").bounds();
                    assert_eq!(timestamp.size.width, px(176.));
                    assert_eq!(severity.size.width, px(88.));
                    assert_eq!(timestamp.right() + px(12.), severity.left());
                    assert_eq!(severity.right() + px(12.), message.left());
                    assert!(message.size.width > timestamp.size.width);
                })?;
            }
            if selector == "tab-Settings" {
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    assert!(window.try_find("sensitivity").is_some());
                    assert!(window.try_find("same-scene-threshold").is_some());
                    assert!(window.try_find("asc-recall-interval").is_some());
                })?;
                cx.update_window(window.into(), |_, window, cx| {
                    window.scroll(
                        "settings-view",
                        ScrollDelta::Pixels(point(px(0.), px(-600.))),
                        cx,
                    );
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
                let dispatched_before_unchanged_rename = observed_dispatcher.dispatched_count();
                cx.update_window(window.into(), |_, window, cx| {
                    assert!(gpui_kit::base::active_focus_trap(window, cx).is_some());
                    window.click("rename-cue-list-33333333-3333-4333-8333-333333333333", cx);
                    window.render_frame(cx);
                    assert!(window.try_find("cue-list-name").is_some());
                    assert!(window.try_find("cue-list-name-editor").is_none());
                    assert!(window.focused(cx).is_some());
                    window.click("submit-cue-list-name", cx);
                    window.press("escape", cx);
                })?;
                cx.run_until_parked();
                anyhow::ensure!(
                    observed_dispatcher.dispatched_count() == dispatched_before_unchanged_rename,
                    "unchanged inline cue-list rename dispatched"
                );
                cx.update_window(window.into(), |_, window, cx| {
                    window.render_frame(cx);
                    assert!(window.try_find("cue-list-name").is_none());
                    assert!(gpui_kit::base::active_focus_trap(window, cx).is_some());
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
        for image in [&connection, &session_menu]
            .into_iter()
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
            ready != safe && ready != session_menu,
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
                "native-connection-connected",
                visual_signature!(connected_connection),
                include_bytes!("visual_snapshots/native-connection-connected.rgb").as_slice(),
            ),
            (
                "native-shell-ready",
                visual_signature!(ready),
                include_bytes!("visual_snapshots/native-shell-ready.rgb").as_slice(),
            ),
            (
                "native-session-menu",
                visual_signature!(session_menu),
                include_bytes!("visual_snapshots/native-session-menu.rgb").as_slice(),
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
        let current_entry_id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let next_entry_id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
        let pending_entry_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();
        let neutral_entry_id = Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap();
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
                entries: vec![
                    CueEntry {
                        id: current_entry_id,
                        scene_internal_id: scene_a,
                    },
                    CueEntry {
                        id: next_entry_id,
                        scene_internal_id: scene_b,
                    },
                    CueEntry {
                        id: pending_entry_id,
                        scene_internal_id: scene_a,
                    },
                    CueEntry {
                        id: neutral_entry_id,
                        scene_internal_id: scene_a,
                    },
                ],
            }],
            active_cue_list_id: Some(list_id.to_string()),
            current_cue_entry_id: Some(current_entry_id.to_string()),
            cued_cue_entry_id: Some(next_entry_id.to_string()),
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
    use super::state::GoSubmissionGuard;
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
                    let go_submissions = Rc::new(RefCell::new(GoSubmissionGuard::default()));
                    let scenes = cx
                        .new(|cx| ScenesView::new(initial.clone(), dispatcher.clone(), window, cx));
                    let cues = cx.new(|cx| {
                        CueListsView::new(
                            initial.clone(),
                            dispatcher.clone(),
                            go_submissions.clone(),
                            window,
                            cx,
                        )
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
                            go_submissions,
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
        use gpui_kit::component::button::ButtonVariants as _;
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
                    super::button::bordered_button("gallery-ready-state")
                        .primary()
                        .label("READY STATE")
                        .disabled(!self.safe)
                        .on_click(cx.listener(|this, _, _, cx| this.show_state(false, cx))),
                )
                .child(
                    super::button::bordered_button("gallery-safe-state")
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

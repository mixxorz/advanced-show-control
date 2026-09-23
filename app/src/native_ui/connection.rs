use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::base::{Button as BaseButton, FocusTrapElement as _};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{IconName, Sizable as _};
use gpui_kit::{
    Context, FocusHandle, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div, prelude::FluentBuilder as _, px, rgb,
};

use crate::connection_state::{DiscoveredLv1Status, DiscoveredLv1System, Lv1SystemIdentity};
use crate::lv1::TcpConnectProbeResult;
use crate::projector::{AppConnectionState, AppViewState};

use super::CommandDispatcher;
use super::button::bordered_button;
use super::theme::{
    CONSOLE_CONTROL, CONSOLE_LINE, CONSOLE_LINE_STRONG, CONSOLE_MUTED, CONSOLE_PANEL,
    CONSOLE_SECONDARY, CONSOLE_SECTION, STATUS_CUED, STATUS_CURRENT, STATUS_DANGER,
};

const LATENCY_COLUMN_WIDTH: f32 = 128.;
const STATUS_COLUMN_WIDTH: f32 = 116.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionDialogMode {
    Startup,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LatencyState {
    Pending { attempt_id: u64 },
    Success(u64),
    Error(String),
}

#[derive(Default)]
pub struct ConnectionState {
    pub mode: Option<ConnectionDialogMode>,
    pub pending_identity: Option<Lv1SystemIdentity>,
    pub pending_command_id: Option<u64>,
    pub command_error: Option<String>,
    latency_session_id: u64,
    next_latency_attempt_id: u64,
    in_flight_latency: HashMap<String, u64>,
    latency: HashMap<String, LatencyState>,
}

impl ConnectionState {
    pub fn startup() -> Self {
        Self {
            mode: Some(ConnectionDialogMode::Startup),
            ..Self::default()
        }
    }

    pub fn is_visible(&self) -> bool {
        self.mode.is_some()
    }

    pub fn open_manual(&mut self) {
        self.invalidate_latency_session();
        self.command_error = None;
        self.mode = Some(ConnectionDialogMode::Manual);
    }

    pub fn finish_command(&mut self, command_id: u64) {
        if self.pending_command_id == Some(command_id) {
            self.pending_identity = None;
            self.pending_command_id = None;
        }
    }

    pub fn close(&mut self) {
        self.invalidate_latency_session();
        self.mode = None;
    }

    pub fn close_startup_if_connected(&mut self, snapshot: &AppViewState) {
        if self.mode == Some(ConnectionDialogMode::Startup)
            && snapshot.connection == AppConnectionState::Connected
        {
            self.close();
        }
    }

    /// @cc [owner:mixxorz,label:safety;presentation] latency-result-active-pending-only
    /// A latency result MUST update presentation state only when its session, exact identity, and
    /// unique attempt ID match both the in-flight owner and the identity's displayed pending attempt.
    /// A matching result received while the identity is absent MUST clear in-flight ownership without
    /// restoring display state. Closed-session, stale-session, superseded-attempt, duplicate, and
    /// unsolicited results MUST leave latency state unchanged.
    pub fn set_latency(
        &mut self,
        session_id: u64,
        attempt_id: u64,
        identity: &Lv1SystemIdentity,
        result: Result<TcpConnectProbeResult, String>,
    ) {
        let key = identity_key(identity);
        if self.mode.is_none()
            || session_id != self.latency_session_id
            || self.in_flight_latency.get(&key) != Some(&attempt_id)
        {
            return;
        }
        self.in_flight_latency.remove(&key);
        if self.latency.get(&key) != Some(&LatencyState::Pending { attempt_id }) {
            return;
        }
        self.latency.insert(
            key,
            match result {
                Ok(result) => LatencyState::Success(result.tcp_connect_ms),
                Err(error) => LatencyState::Error(error),
            },
        );
    }

    pub fn begin_latency(&mut self, identity: &Lv1SystemIdentity) -> Option<(u64, u64)> {
        let key = identity_key(identity);
        if self.mode.is_none() || self.latency.contains_key(&key) {
            return None;
        }
        if let Some(&attempt_id) = self.in_flight_latency.get(&key) {
            self.latency
                .insert(key, LatencyState::Pending { attempt_id });
            return None;
        }
        let attempt_id = self.next_latency_attempt_id;
        self.next_latency_attempt_id = self
            .next_latency_attempt_id
            .checked_add(1)
            .expect("connection latency attempt identifier exhausted");
        self.in_flight_latency.insert(key.clone(), attempt_id);
        self.latency
            .insert(key, LatencyState::Pending { attempt_id });
        Some((self.latency_session_id, attempt_id))
    }

    /// @cc [owner:mixxorz,label:product;presentation] discovered-latency-session-reconciliation
    /// While the dialog is open, reconciliation MUST prune displayed state for absent identities but
    /// retain their in-flight ownership. Reappearance before completion MUST restore the same pending
    /// attempt without dispatching another probe. A completion received while absent MUST remain hidden
    /// and permit a fresh attempt after reappearance. While closed, reconciliation MUST return no probes
    /// and retain neither displayed nor in-flight latency state.
    pub fn reconcile_latency_probes(
        &mut self,
        systems: &[DiscoveredLv1System],
    ) -> Vec<(u64, u64, Lv1SystemIdentity)> {
        if self.mode.is_none() {
            self.latency.clear();
            self.in_flight_latency.clear();
            return Vec::new();
        }

        let discovered_keys = systems
            .iter()
            .map(|system| identity_key(&system.identity))
            .collect::<HashSet<_>>();
        self.latency
            .retain(|identity, _| discovered_keys.contains(identity));

        systems
            .iter()
            .filter_map(|system| {
                self.begin_latency(&system.identity)
                    .map(|(session_id, attempt_id)| {
                        (session_id, attempt_id, system.identity.clone())
                    })
            })
            .collect()
    }

    fn invalidate_latency_session(&mut self) {
        self.latency_session_id = self
            .latency_session_id
            .checked_add(1)
            .expect("connection latency session identifier exhausted");
        self.latency.clear();
        self.in_flight_latency.clear();
    }
}

pub(super) fn begin_automatic_latency_probes(
    state: &Rc<RefCell<ConnectionState>>,
    systems: &[DiscoveredLv1System],
    mut dispatch: impl FnMut(u64, u64, Lv1SystemIdentity),
) {
    for (session_id, attempt_id, identity) in state.borrow_mut().reconcile_latency_probes(systems) {
        dispatch(session_id, attempt_id, identity);
    }
}

pub(super) fn apply_latency_result(
    state: &Rc<RefCell<ConnectionState>>,
    session_id: u64,
    attempt_id: u64,
    identity: &Lv1SystemIdentity,
    result: Result<TcpConnectProbeResult, String>,
    window: &mut Window,
) {
    state
        .borrow_mut()
        .set_latency(session_id, attempt_id, identity, result);
    window.refresh();
}

/// @cc [owner:mixxorz,label:accessibility;keyboard] connection-modal-focus
/// While visible, the connection chooser MUST expose a named dialog over an interaction-blocking
/// backdrop, own and trap keyboard focus, consume Escape, and restore the prior action focus when
/// dismissed, falling back to AppRoot action focus when no prior focus exists. Reopening MUST
/// establish a fresh latency-probe presentation session.
#[derive(Clone)]
pub(super) struct ConnectionFocusRestore {
    return_focus: Rc<RefCell<Option<FocusHandle>>>,
    fallback_focus: FocusHandle,
}

impl ConnectionFocusRestore {
    pub(super) fn new(
        return_focus: Rc<RefCell<Option<FocusHandle>>>,
        fallback_focus: FocusHandle,
    ) -> Self {
        Self {
            return_focus,
            fallback_focus,
        }
    }
}

pub(super) fn render_connection_overlay<T: 'static>(
    snapshot: AppViewState,
    state: Rc<RefCell<ConnectionState>>,
    dispatcher: CommandDispatcher,
    modal_focus: &FocusHandle,
    focus_restore: ConnectionFocusRestore,
    cx: &mut Context<T>,
) -> impl IntoElement {
    let connection = state.borrow();
    let latencies = connection.latency.clone();
    let pending_identity = connection.pending_identity.clone();
    let command_error = connection.command_error.clone();
    drop(connection);

    let connected = snapshot.connected_lv1_identity.clone();
    let rows = snapshot.discovered_lv1_systems.clone();
    let close_state = state.clone();
    let close_focus_restore = focus_restore.clone();
    let close = bordered_button("close-connection")
        .small()
        .icon(IconName::Close)
        .accessibility_label("Close connection chooser")
        .on_click(move |_, window, cx| {
            close_connection_overlay(&close_state, &close_focus_restore, window, cx);
        });
    let disconnect_dispatcher = dispatcher.clone();
    let disconnect_state = state.clone();

    let mut body = div().flex().flex_col().gap_2().max_h(px(420.));
    if let Some(error) = command_error {
        body = body.child(
            div()
                .p_3()
                .border_1()
                .border_color(rgb(STATUS_DANGER))
                .bg(rgb(CONSOLE_CONTROL))
                .text_color(rgb(STATUS_DANGER))
                .child(error),
        );
    }
    body = body.child(
        div()
            .flex()
            .items_center()
            .gap_3()
            .p_2()
            .border_b_1()
            .border_color(rgb(CONSOLE_LINE))
            .text_xs()
            .text_color(rgb(CONSOLE_SECONDARY))
            .child(div().w(px(3.)))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .child(div().flex_1().child("CONSOLE"))
                    .child(
                        div()
                            .id("connection-heading-latency")
                            .test_support()
                            .w(px(LATENCY_COLUMN_WIDTH))
                            .text_right()
                            .child("LATENCY"),
                    )
                    .child(
                        div()
                            .id("connection-heading-status")
                            .test_support()
                            .w(px(STATUS_COLUMN_WIDTH))
                            .text_right()
                            .child("STATUS"),
                    ),
            ),
    );
    if rows.is_empty() {
        body = body.child(
            div()
                .p_5()
                .border_1()
                .border_color(rgb(CONSOLE_LINE))
                .bg(rgb(CONSOLE_SECTION))
                .text_color(rgb(CONSOLE_SECONDARY))
                .child("Searching for consoles…"),
        );
    } else {
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .overflow_y_scrollbar()
                .children(rows.iter().map(|system| {
                    system_row(
                        system,
                        connected.as_ref(),
                        pending_identity.as_ref(),
                        latencies.get(&identity_key(&system.identity)),
                        state.clone(),
                        focus_restore.clone(),
                        dispatcher.clone(),
                    )
                })),
        );
    }

    div()
        .id("connection-overlay")
        .role(Role::Dialog)
        .aria_label("Connect to LV1")
        .test_support()
        .absolute()
        .inset_0()
        .bg(rgb(0x000000).opacity(0.82))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_key_down(
            cx.listener(move |_, event: &gpui_kit::KeyDownEvent, window, cx| {
                if super::keyboard::normalized_physical_key(&event.keystroke) == "Escape" {
                    close_connection_overlay(&state, &focus_restore, window, cx);
                    cx.stop_propagation();
                }
            }),
        )
        .focus_trap("connection-focus-trap", modal_focus)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id("connection-modal")
                .test_support()
                .w(px(620.))
                .max_h(px(560.))
                .flex()
                .flex_col()
                .gap_3()
                .p_5()
                .bg(rgb(CONSOLE_PANEL))
                .border_1()
                .border_color(rgb(CONSOLE_LINE_STRONG))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_lg().child("CONNECT TO LV1"))
                        .child(close),
                )
                .child(body)
                .when(
                    snapshot.connection == AppConnectionState::Connected,
                    |panel| {
                        panel.child(
                            div().flex().justify_end().child(
                                bordered_button("disconnect-lv1")
                                    .danger()
                                    .label("DISCONNECT")
                                    .on_click(move |_, window, _| {
                                        disconnect_dispatcher.dispatch(|commands| async move {
                                            commands.disconnect_lv1().await.map(|_| ())
                                        });
                                        disconnect_state.borrow_mut().open_manual();
                                        window.refresh();
                                    }),
                            ),
                        )
                    },
                ),
        )
}

fn close_connection_overlay(
    state: &Rc<RefCell<ConnectionState>>,
    focus_restore: &ConnectionFocusRestore,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) {
    state.borrow_mut().close();
    if let Some(focus) = focus_restore.return_focus.borrow_mut().take() {
        focus.focus(window, cx);
    } else {
        focus_restore.fallback_focus.focus(window, cx);
    }
    window.refresh();
}

fn latency_text(latency: Option<&LatencyState>) -> String {
    match latency {
        None | Some(LatencyState::Pending { .. }) => "Testing…".to_string(),
        Some(LatencyState::Success(ms)) => format!("{ms} ms"),
        Some(LatencyState::Error(error)) => format!("Failed: {error}"),
    }
}

fn system_accessibility_label(
    display_name: &str,
    connection_status: &str,
    latency: Option<&LatencyState>,
) -> String {
    format!(
        "{display_name}, {connection_status}, latency {}",
        latency_text(latency)
    )
}

fn system_row(
    system: &DiscoveredLv1System,
    connected: Option<&Lv1SystemIdentity>,
    pending: Option<&Lv1SystemIdentity>,
    latency: Option<&LatencyState>,
    state: Rc<RefCell<ConnectionState>>,
    focus_restore: ConnectionFocusRestore,
    dispatcher: CommandDispatcher,
) -> impl IntoElement {
    let identity = system.identity.clone();
    let display_name: SharedString = identity
        .host
        .clone()
        .unwrap_or_else(|| "LV1 Console".into())
        .into();
    let is_connected = identities_match(&identity, connected);
    let unavailable = system.status == DiscoveredLv1Status::Unavailable;
    let selecting = identities_match(&identity, pending);
    let status = if unavailable {
        "Unavailable"
    } else if is_connected {
        "Connected"
    } else if selecting {
        "Connecting…"
    } else {
        "Available"
    };
    let status_color = if unavailable {
        STATUS_DANGER
    } else if is_connected {
        STATUS_CURRENT
    } else {
        STATUS_CUED
    };
    let accessibility_label = system_accessibility_label(&display_name, status, latency);
    let latency_text = latency_text(latency);
    let select_dispatcher = dispatcher.clone();
    let select_identity = identity.clone();
    let select_state = state;
    let resume_state = select_state.clone();
    let resume_focus_restore = focus_restore;

    div()
        .flex()
        .items_center()
        .gap_3()
        .p_2()
        .border_1()
        .border_color(rgb(CONSOLE_LINE))
        .bg(rgb(CONSOLE_PANEL))
        .child(
            div()
                .id(format!("connection-marker-{}", identity_key(&identity)))
                .test_support()
                .w(px(3.))
                .self_stretch()
                .when(is_connected, |bar| bar.bg(rgb(STATUS_CURRENT))),
        )
        .child(
            BaseButton::new(SharedString::from(format!(
                "select-system-{}",
                identity_key(&identity)
            )))
            .accessibility_label(accessibility_label)
            .disabled(unavailable || (!is_connected && pending.is_some()))
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_2()
            .py_1()
            .when(!unavailable && !is_connected && pending.is_none(), |row| {
                row.on_click(move |_, window, _| {
                    let identity = select_identity.clone();
                    let mut state = select_state.borrow_mut();
                    if state.pending_identity.is_some() {
                        return;
                    }
                    state.pending_identity = Some(identity.clone());
                    drop(state);
                    let command_id = select_dispatcher.dispatch(move |commands| async move {
                        commands.connect_lv1_system(identity).await.map(|_| ())
                    });
                    select_state.borrow_mut().pending_command_id = Some(command_id);
                    window.refresh();
                })
            })
            .when(is_connected, |row| {
                row.on_click(move |_, window, cx| {
                    close_connection_overlay(&resume_state, &resume_focus_restore, window, cx);
                })
            })
            .child(
                div()
                    .id(format!("connection-name-{}", identity_key(&identity)))
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(display_name)
                    .child(
                        div()
                            .font_family("Fira Code")
                            .text_sm()
                            .text_color(rgb(CONSOLE_MUTED))
                            .child(format!("{}:{}", identity.address, identity.port)),
                    ),
            )
            .child(
                div()
                    .id(format!("connection-latency-{}", identity_key(&identity)))
                    .test_support()
                    .w(px(LATENCY_COLUMN_WIDTH))
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_right()
                    .text_sm()
                    .text_color(rgb(match latency {
                        Some(LatencyState::Error(_)) => STATUS_DANGER,
                        _ => CONSOLE_SECONDARY,
                    }))
                    .child(latency_text),
            )
            .child(
                div()
                    .id(format!("connection-status-{}", identity_key(&identity)))
                    .test_support()
                    .w(px(STATUS_COLUMN_WIDTH))
                    .text_right()
                    .text_color(rgb(status_color))
                    .child(status),
            ),
        )
}

pub fn identities_match(left: &Lv1SystemIdentity, right: Option<&Lv1SystemIdentity>) -> bool {
    let Some(right) = right else { return false };
    match (&left.uuid, &right.uuid) {
        (Some(left), Some(right)) => left == right,
        _ => left.host == right.host && left.address == right.address && left.port == right.port,
    }
}

pub fn identity_key(identity: &Lv1SystemIdentity) -> String {
    format!(
        "{}\0{}\0{}\0{}",
        identity.uuid.as_deref().unwrap_or_default(),
        identity.host.as_deref().unwrap_or_default(),
        identity.address,
        identity.port
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(
        uuid: Option<&str>,
        host: Option<&str>,
        address: &str,
        port: u16,
    ) -> Lv1SystemIdentity {
        Lv1SystemIdentity {
            uuid: uuid.map(str::to_string),
            host: host.map(str::to_string),
            address: address.to_string(),
            port,
        }
    }

    #[test]
    fn identity_matching_prefers_uuid_then_requires_the_complete_endpoint() {
        let connected = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        assert!(identities_match(
            &identity(Some("a"), Some("Other"), "x", 9),
            Some(&connected)
        ));
        assert!(!identities_match(
            &identity(Some("b"), Some("FOH"), "10.0.0.1", 1234),
            Some(&connected)
        ));

        let connected = identity(None, Some("FOH"), "10.0.0.1", 1234);
        assert!(identities_match(
            &identity(None, Some("FOH"), "10.0.0.1", 1234),
            Some(&connected)
        ));
        assert!(!identities_match(
            &identity(None, Some("FOH"), "10.0.0.2", 1234),
            Some(&connected)
        ));
    }

    #[test]
    fn startup_dialog_closes_only_after_a_connected_snapshot() {
        let mut state = ConnectionState::startup();
        state.close_startup_if_connected(&AppViewState::default());
        assert!(state.is_visible());

        let connected = AppViewState {
            connection: AppConnectionState::Connected,
            ..Default::default()
        };
        state.close_startup_if_connected(&connected);
        assert!(!state.is_visible());

        state.open_manual();
        state.close_startup_if_connected(&connected);
        assert!(state.is_visible());
    }

    #[test]
    fn only_the_matching_command_releases_a_pending_connection() {
        let mut state = ConnectionState {
            pending_identity: Some(identity(Some("a"), Some("FOH"), "10.0.0.1", 1234)),
            pending_command_id: Some(7),
            ..Default::default()
        };

        state.finish_command(6);
        assert!(state.pending_identity.is_some());
        assert_eq!(state.pending_command_id, Some(7));

        state.finish_command(7);
        assert!(state.pending_identity.is_none());
        assert_eq!(state.pending_command_id, None);
    }

    #[test]
    fn opening_the_dialog_discards_a_previous_connection_error() {
        let mut state = ConnectionState::startup();
        state.command_error = Some("old failure".to_string());

        state.open_manual();

        assert_eq!(state.command_error, None);
    }

    #[test]
    fn reopening_the_dialog_discards_previous_latency_results() {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let mut state = ConnectionState {
            mode: Some(ConnectionDialogMode::Manual),
            ..Default::default()
        };
        let (session_id, attempt_id) = state.begin_latency(&console).unwrap();
        state.set_latency(
            session_id,
            attempt_id,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
        );
        assert!(!state.latency.is_empty());

        state.open_manual();

        assert!(state.latency.is_empty());
    }

    #[test]
    fn a_probe_result_from_a_closed_dialog_cannot_enter_a_new_session() {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let mut state = ConnectionState::startup();
        let (old_session, old_attempt) = state.begin_latency(&console).unwrap();

        state.close();
        state.open_manual();
        let (new_session, new_attempt) = state.begin_latency(&console).unwrap();
        assert_ne!(old_session, new_session);
        assert_ne!(old_attempt, new_attempt);
        state.set_latency(
            old_session,
            old_attempt,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
        );

        assert_eq!(
            state.latency.get(&identity_key(&console)),
            Some(&LatencyState::Pending {
                attempt_id: new_attempt
            })
        );
    }

    #[test]
    fn visible_dialog_starts_each_discovered_identity_once_per_session() {
        let first = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let second = identity(Some("b"), Some("MON"), "10.0.0.2", 1234);
        let systems = vec![
            DiscoveredLv1System {
                identity: first.clone(),
                status: DiscoveredLv1Status::Available,
            },
            DiscoveredLv1System {
                identity: second.clone(),
                status: DiscoveredLv1Status::Available,
            },
        ];
        let mut state = ConnectionState::startup();

        let probes = state.reconcile_latency_probes(&systems);
        assert_eq!(probes.len(), 2);
        assert_eq!(probes[0].2, first);
        assert_eq!(probes[1].2, second);
        assert_ne!(probes[0].1, probes[1].1);
        assert!(state.reconcile_latency_probes(&systems).is_empty());
    }

    #[test]
    fn latency_reconciliation_prunes_removed_identities_and_rejects_their_results() {
        let removed = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let retained = identity(Some("b"), Some("MON"), "10.0.0.2", 1234);
        let mut state = ConnectionState::startup();
        let systems = |identities: &[Lv1SystemIdentity]| {
            identities
                .iter()
                .cloned()
                .map(|identity| DiscoveredLv1System {
                    identity,
                    status: DiscoveredLv1Status::Available,
                })
                .collect::<Vec<_>>()
        };

        let probes = state.reconcile_latency_probes(&systems(&[removed.clone(), retained.clone()]));
        let (session_id, attempt_id, _) = &probes[0];
        assert!(
            state
                .reconcile_latency_probes(&systems(std::slice::from_ref(&retained)))
                .is_empty()
        );

        state.set_latency(
            *session_id,
            *attempt_id,
            &removed,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
        );

        assert!(!state.latency.contains_key(&identity_key(&removed)));
        assert_eq!(
            state.latency.get(&identity_key(&retained)),
            Some(&LatencyState::Pending {
                attempt_id: probes[1].1
            })
        );
    }

    #[test]
    fn remove_then_reappear_before_completion_reuses_the_single_in_flight_attempt() {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let systems = |present: bool| {
            present
                .then(|| DiscoveredLv1System {
                    identity: console.clone(),
                    status: DiscoveredLv1Status::Available,
                })
                .into_iter()
                .collect::<Vec<_>>()
        };
        let mut state = ConnectionState::startup();

        let probes = state.reconcile_latency_probes(&systems(true));
        let (session_id, attempt_id, _) = &probes[0];
        assert!(state.reconcile_latency_probes(&systems(false)).is_empty());
        assert!(!state.latency.contains_key(&identity_key(&console)));
        assert!(state.reconcile_latency_probes(&systems(true)).is_empty());
        assert_eq!(
            state.latency.get(&identity_key(&console)),
            Some(&LatencyState::Pending {
                attempt_id: *attempt_id
            })
        );

        state.set_latency(
            *session_id,
            *attempt_id,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
        );
        assert_eq!(
            state.latency.get(&identity_key(&console)),
            Some(&LatencyState::Success(12))
        );
    }

    #[test]
    fn completion_while_removed_stays_hidden_and_permits_a_fresh_attempt_on_reappearance() {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let system = DiscoveredLv1System {
            identity: console.clone(),
            status: DiscoveredLv1Status::Available,
        };
        let mut state = ConnectionState::startup();

        let first = state.reconcile_latency_probes(std::slice::from_ref(&system));
        let (session_id, first_attempt_id, _) = &first[0];
        state.reconcile_latency_probes(&[]);
        state.set_latency(
            *session_id,
            *first_attempt_id,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 99 }),
        );
        assert!(!state.latency.contains_key(&identity_key(&console)));

        let second = state.reconcile_latency_probes(&[system]);
        assert_eq!(second.len(), 1);
        assert_ne!(second[0].1, *first_attempt_id);
    }

    #[test]
    fn reopening_clears_hidden_in_flight_attempts() {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let system = DiscoveredLv1System {
            identity: console,
            status: DiscoveredLv1Status::Available,
        };
        let mut state = ConnectionState::startup();

        let first = state.reconcile_latency_probes(std::slice::from_ref(&system));
        state.reconcile_latency_probes(&[]);
        state.close();
        state.open_manual();
        let reopened = state.reconcile_latency_probes(&[system]);

        assert_eq!(reopened.len(), 1);
        assert_ne!(reopened[0].0, first[0].0);
        assert_ne!(reopened[0].1, first[0].1);
    }

    #[test]
    fn latency_results_require_a_currently_pending_identity() {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let mut state = ConnectionState::startup();

        state.set_latency(
            0,
            0,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
        );
        assert!(state.latency.is_empty());

        let (session_id, attempt_id) = state.begin_latency(&console).unwrap();
        state.set_latency(
            session_id,
            attempt_id,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
        );
        state.set_latency(
            session_id,
            attempt_id,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 99 }),
        );

        assert_eq!(
            state.latency.get(&identity_key(&console)),
            Some(&LatencyState::Success(12))
        );
    }

    #[test]
    fn latency_text_covers_testing_success_and_failure_states() {
        assert_eq!(latency_text(None), "Testing…");
        assert_eq!(
            latency_text(Some(&LatencyState::Pending { attempt_id: 7 })),
            "Testing…"
        );
        assert_eq!(latency_text(Some(&LatencyState::Success(12))), "12 ms");
        assert_eq!(
            latency_text(Some(&LatencyState::Error("timed out".to_string()))),
            "Failed: timed out"
        );
    }

    #[test]
    fn row_accessibility_label_includes_latency_status() {
        assert_eq!(
            system_accessibility_label("FOH", "Available", Some(&LatencyState::Success(12))),
            "FOH, Available, latency 12 ms"
        );
        assert_eq!(
            system_accessibility_label(
                "FOH",
                "Available",
                Some(&LatencyState::Error("timed out".to_string()))
            ),
            "FOH, Available, latency Failed: timed out"
        );
    }

    #[gpui_kit::test]
    fn modal_probes_render_and_reopen_deterministically_while_restoring_focus_and_blocking_pointer(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use std::sync::{Arc, Mutex};

        use gpui_kit::component::Root;
        use gpui_kit::test::TestWindowExt as _;
        use gpui_kit::{AppContext as _, Context, Render, SharedString, point, size};

        struct Harness {
            snapshot: AppViewState,
            state: Rc<RefCell<ConnectionState>>,
            dispatcher: CommandDispatcher,
            modal_focus: FocusHandle,
            background_focus: FocusHandle,
            return_focus: Rc<RefCell<Option<FocusHandle>>>,
            background_clicks: Rc<RefCell<usize>>,
        }

        impl Render for Harness {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let systems = self.snapshot.discovered_lv1_systems.clone();
                let dispatcher = self.dispatcher.clone();
                begin_automatic_latency_probes(
                    &self.state,
                    &systems,
                    move |session_id, attempt_id, identity| {
                        dispatcher.probe_latency(session_id, attempt_id, identity, None);
                    },
                );
                let clicks = self.background_clicks.clone();
                div()
                    .relative()
                    .size_full()
                    .child(
                        BaseButton::new("connection-test-background")
                            .track_focus(&self.background_focus)
                            .size_full()
                            .on_click(move |_, _, _| *clicks.borrow_mut() += 1),
                    )
                    .when(self.state.borrow().is_visible(), |root| {
                        root.child(render_connection_overlay(
                            self.snapshot.clone(),
                            self.state.clone(),
                            self.dispatcher.clone(),
                            &self.modal_focus,
                            ConnectionFocusRestore::new(
                                self.return_focus.clone(),
                                self.background_focus.clone(),
                            ),
                            cx,
                        ))
                    })
            }
        }

        struct TestDir(std::path::PathBuf);
        impl Drop for TestDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let settings_dir = TestDir(std::env::temp_dir().join(format!(
            "asc-connection-modal-test-{}",
            uuid::Uuid::new_v4()
        )));
        let probes = Arc::new(Mutex::new(Vec::new()));
        let dispatcher = {
            let _entered = runtime.enter();
            let events = crate::runtime::events::AppEventBus::default();
            let (show, show_task, show_peers, lockout) =
                crate::show::build_show_actor(events.clone());
            let (settings, settings_task, _) =
                crate::settings::build_settings_actor(settings_dir.0.clone(), events.clone());
            let lifecycle = crate::lifecycle::AppLifecycle::new(
                events,
                show.clone(),
                show_peers,
                lockout,
                settings.clone(),
            );
            show_task.spawn();
            settings_task.spawn();
            let (ui_logs, _) = tokio::sync::broadcast::channel(8);
            let commands = crate::application::ApplicationCommandContext::new(
                lifecycle, show, settings, ui_logs,
            );
            let (ui_events, _) = tokio::sync::mpsc::unbounded_channel();
            let captured = probes.clone();
            CommandDispatcher::new(runtime.handle().clone(), commands, ui_events)
                .with_latency_probe_override(move |session, attempt, identity, timeout| {
                    captured
                        .lock()
                        .unwrap()
                        .push((session, attempt, identity, timeout));
                })
        };

        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let state = Rc::new(RefCell::new(ConnectionState::startup()));
        let return_focus = Rc::new(RefCell::new(None));
        let background_clicks = Rc::new(RefCell::new(0));
        let focus_handles = Rc::new(RefCell::new(None));
        cx.update(gpui_kit::init);
        let handle = cx.open_window(size(px(720.), px(520.)), |window, cx| {
            let modal_focus = cx.focus_handle();
            let background_focus = cx.focus_handle();
            background_focus.focus(window, cx);
            *return_focus.borrow_mut() = Some(background_focus.clone());
            *focus_handles.borrow_mut() = Some((background_focus.clone(), modal_focus.clone()));
            modal_focus.focus(window, cx);
            let harness = cx.new(|_| Harness {
                snapshot: AppViewState {
                    discovered_lv1_systems: vec![DiscoveredLv1System {
                        identity: console.clone(),
                        status: DiscoveredLv1Status::Available,
                    }],
                    ..Default::default()
                },
                state: state.clone(),
                dispatcher,
                modal_focus,
                background_focus,
                return_focus: return_focus.clone(),
                background_clicks: background_clicks.clone(),
            });
            Root::new(harness, window, cx)
        });

        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            let row_id = SharedString::from(format!("select-system-{}", identity_key(&console)));
            assert_eq!(
                window.find(row_id.clone()).label(),
                Some("FOH, Available, latency Testing…")
            );
            let key = identity_key(&console);
            let marker = window.find(format!("connection-marker-{key}")).bounds();
            let name = window.find(format!("connection-name-{key}")).bounds();
            let latency = window.find(format!("connection-latency-{key}")).bounds();
            let status = window.find(format!("connection-status-{key}")).bounds();
            assert_eq!(marker.size.width, px(3.));
            assert!(marker.right() < name.left());
            assert!(name.right() < latency.left());
            assert!(latency.right() < status.left());
            assert!(
                (window.find("connection-heading-latency").bounds().right() - latency.right())
                    .abs()
                    <= px(1.)
            );
            assert!(
                (window.find("connection-heading-status").bounds().right() - status.right()).abs()
                    <= px(1.)
            );
            assert_eq!(probes.lock().unwrap().len(), 1);
            let (old_session, old_attempt, _, timeout) = probes.lock().unwrap()[0].clone();
            assert_eq!(timeout, None);

            apply_latency_result(
                &state,
                old_session,
                old_attempt,
                &console,
                Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
                window,
            );
            window.render_frame(cx);
            assert_eq!(
                window.find(row_id.clone()).label(),
                Some("FOH, Available, latency 12 ms")
            );

            window.click_at("connection-focus-trap", point(px(5.), px(5.)), cx);
            assert_eq!(*background_clicks.borrow(), 0);
            window.click("close-connection", cx);
            window.render_frame(cx);
            let (background_focus, modal_focus) = focus_handles.borrow().clone().unwrap();
            assert!(background_focus.is_focused(window));

            apply_latency_result(
                &state,
                old_session,
                old_attempt,
                &console,
                Ok(TcpConnectProbeResult { tcp_connect_ms: 99 }),
                window,
            );
            assert!(state.borrow().latency.is_empty());

            state.borrow_mut().open_manual();
            *return_focus.borrow_mut() = Some(background_focus.clone());
            modal_focus.focus(window, cx);
            window.refresh();
            window.render_frame(cx);
            assert_eq!(
                window.find(row_id).label(),
                Some("FOH, Available, latency Testing…")
            );
            let probes = probes.lock().unwrap();
            assert_eq!(probes.len(), 2);
            assert_ne!(probes[0].0, probes[1].0);
            assert_ne!(probes[0].1, probes[1].1);
            drop(probes);

            window.press("escape", cx);
            window.render_frame(cx);
            assert!(background_focus.is_focused(window));

            state.borrow_mut().open_manual();
            *return_focus.borrow_mut() = None;
            modal_focus.focus(window, cx);
            window.refresh();
            window.render_frame(cx);
            window.click("close-connection", cx);
            window.render_frame(cx);
            assert!(background_focus.is_focused(window));
        })
        .unwrap();
    }

    #[test]
    fn closed_dialog_does_not_start_automatic_probes() {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let systems = vec![DiscoveredLv1System {
            identity: console,
            status: DiscoveredLv1Status::Available,
        }];
        let mut state = ConnectionState::default();

        assert!(state.reconcile_latency_probes(&systems).is_empty());
        assert!(state.latency.is_empty());
    }
}

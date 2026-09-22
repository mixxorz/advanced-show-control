use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::base::Button as BaseButton;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::dialog::{Dialog, DialogContent};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::{
    App, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
    prelude::FluentBuilder as _, px, rgb,
};

use crate::connection_state::{DiscoveredLv1Status, DiscoveredLv1System, Lv1SystemIdentity};
use crate::lv1::TcpConnectProbeResult;
use crate::projector::{AppConnectionState, AppViewState};

use super::CommandDispatcher;
use super::button::bordered_button;
use super::theme::{
    CONSOLE_CONTROL, CONSOLE_LINE, CONSOLE_MUTED, CONSOLE_PANEL, CONSOLE_SECONDARY, STATUS_CUED,
    STATUS_CURRENT, STATUS_DANGER,
};

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
    /// unique attempt ID match that identity's current pending attempt. Closed-session, stale-session,
    /// removed-identity, superseded-attempt, duplicate, and unsolicited results MUST leave latency
    /// state unchanged.
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
            || self.latency.get(&key) != Some(&LatencyState::Pending { attempt_id })
        {
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
        let attempt_id = self.next_latency_attempt_id;
        self.next_latency_attempt_id = self
            .next_latency_attempt_id
            .checked_add(1)
            .expect("connection latency attempt identifier exhausted");
        self.latency
            .insert(key, LatencyState::Pending { attempt_id });
        Some((self.latency_session_id, attempt_id))
    }

    /// @cc [owner:mixxorz,label:product;presentation] discovered-latency-session-reconciliation
    /// While the dialog is open, reconciliation MUST prune identities absent from the complete
    /// discovered list and return one probe for each identity not currently tracked in this dialog
    /// session. A pruned identity that reappears MUST receive a new unique attempt ID. While closed,
    /// reconciliation MUST return no probes and retain no latency state.
    pub fn reconcile_latency_probes(
        &mut self,
        systems: &[DiscoveredLv1System],
    ) -> Vec<(u64, u64, Lv1SystemIdentity)> {
        if self.mode.is_none() {
            self.latency.clear();
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

pub fn open_connection_dialog(
    window: &mut Window,
    cx: &mut App,
    snapshot: Rc<RefCell<AppViewState>>,
    state: Rc<RefCell<ConnectionState>>,
    dispatcher: CommandDispatcher,
) {
    let systems = snapshot.borrow().discovered_lv1_systems.clone();
    let probe_dispatcher = dispatcher.clone();
    begin_automatic_latency_probes(&state, &systems, move |session_id, attempt_id, identity| {
        probe_dispatcher.probe_latency(session_id, attempt_id, identity, None);
    });

    window.open_dialog(cx, move |dialog, _, _| {
        let close_state = state.clone();
        let connection = state.borrow();
        let latencies = connection.latency.clone();
        let pending_identity = connection.pending_identity.clone();
        let command_error = connection.command_error.clone();
        drop(connection);
        build_dialog(
            dialog,
            snapshot.borrow().clone(),
            command_error,
            latencies,
            pending_identity,
            state.clone(),
            dispatcher.clone(),
        )
        .on_close(move |_, _, _| close_state.borrow_mut().close())
    });
}

fn build_dialog(
    dialog: Dialog,
    snapshot: AppViewState,
    command_error: Option<String>,
    latencies: HashMap<String, LatencyState>,
    pending_identity: Option<Lv1SystemIdentity>,
    state: Rc<RefCell<ConnectionState>>,
    dispatcher: CommandDispatcher,
) -> Dialog {
    let connected = snapshot.connected_lv1_identity.clone();
    let rows = snapshot.discovered_lv1_systems.clone();
    let disconnect_dispatcher = dispatcher.clone();
    let disconnect_state = state.clone();

    dialog
        .width(px(680.))
        .title("CONNECT TO LV1")
        .content(move |content: DialogContent, _, _| {
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .min_w(px(560.))
                .max_h(px(420.));
            if let Some(error) = command_error.clone() {
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
            if rows.is_empty() {
                body = body.child(
                    div()
                        .p_5()
                        .border_1()
                        .border_color(rgb(CONSOLE_LINE))
                        .bg(rgb(CONSOLE_CONTROL))
                        .text_color(rgb(CONSOLE_SECONDARY))
                        .child("Searching for consoles…"),
                );
            } else {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .overflow_y_scrollbar()
                        .children(rows.iter().map(|system| {
                            system_row(
                                system,
                                connected.as_ref(),
                                pending_identity.as_ref(),
                                latencies.get(&identity_key(&system.identity)),
                                state.clone(),
                                dispatcher.clone(),
                            )
                        })),
                );
            }
            content.child(body)
        })
        .when(
            snapshot.connection == AppConnectionState::Connected,
            |dialog| {
                dialog.footer(
                    div().flex().justify_end().child(
                        bordered_button("disconnect-lv1")
                            .danger()
                            .label("DISCONNECT")
                            .on_click(move |_, window, cx| {
                                disconnect_dispatcher.dispatch(|commands| async move {
                                    commands.disconnect_lv1().await.map(|_| ())
                                });
                                window.close_dialog(cx);
                                disconnect_state.borrow_mut().open_manual();
                            }),
                    ),
                )
            },
        )
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

    div()
        .flex()
        .items_center()
        .gap_3()
        .p_2()
        .border_1()
        .border_color(rgb(if is_connected {
            STATUS_CURRENT
        } else {
            CONSOLE_LINE
        }))
        .bg(rgb(CONSOLE_PANEL))
        .child(
            BaseButton::new(SharedString::from(format!(
                "select-system-{}",
                identity_key(&identity)
            )))
            .accessibility_label(accessibility_label)
            .disabled(unavailable || (!is_connected && pending.is_some()))
            .flex()
            .flex_1()
            .items_center()
            .justify_between()
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
                    resume_state.borrow_mut().close();
                    window.close_dialog(cx);
                })
            })
            .child(
                div().flex().flex_col().child(display_name).child(
                    div()
                        .font_family("Fira Code")
                        .text_sm()
                        .text_color(rgb(CONSOLE_MUTED))
                        .child(format!("{}:{}", identity.address, identity.port)),
                ),
            )
            .child(div().text_color(rgb(status_color)).child(status)),
        )
        .child(
            div()
                .min_w(px(92.))
                .text_sm()
                .text_color(rgb(match latency {
                    Some(LatencyState::Error(_)) => STATUS_DANGER,
                    _ => CONSOLE_SECONDARY,
                }))
                .child(latency_text),
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
    fn reappearing_identity_rejects_the_removed_attempt_result() {
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

        let old_probes = state.reconcile_latency_probes(&systems(true));
        let [(session_id, old_attempt_id, _)] = old_probes.as_slice() else {
            panic!("first discovery must dispatch one probe");
        };
        state.reconcile_latency_probes(&systems(false));
        let new_probes = state.reconcile_latency_probes(&systems(true));
        let [(new_session_id, new_attempt_id, _)] = new_probes.as_slice() else {
            panic!("reappearance must dispatch one probe");
        };

        assert_eq!(session_id, new_session_id);
        assert_ne!(old_attempt_id, new_attempt_id);
        state.set_latency(
            *session_id,
            *old_attempt_id,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 99 }),
        );
        state.set_latency(
            *session_id,
            *new_attempt_id,
            &console,
            Ok(TcpConnectProbeResult { tcp_connect_ms: 12 }),
        );

        assert_eq!(
            state.latency.get(&identity_key(&console)),
            Some(&LatencyState::Success(12))
        );
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
    fn automatic_latency_probes_begin_without_row_interaction(_: &mut gpui_kit::TestAppContext) {
        let console = identity(Some("a"), Some("FOH"), "10.0.0.1", 1234);
        let systems = vec![DiscoveredLv1System {
            identity: console.clone(),
            status: DiscoveredLv1Status::Available,
        }];
        let state = Rc::new(RefCell::new(ConnectionState::startup()));
        let mut dispatched = Vec::new();

        begin_automatic_latency_probes(&state, &systems, |session_id, attempt_id, identity| {
            dispatched.push((session_id, attempt_id, identity));
        });

        assert_eq!(dispatched, vec![(0, 0, console.clone())]);
        assert_eq!(
            state.borrow().latency.get(&identity_key(&console)),
            Some(&LatencyState::Pending { attempt_id: 0 })
        );
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

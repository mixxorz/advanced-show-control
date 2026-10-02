use super::backend::{Backend, BackendError, Candidate, Checked};
use super::*;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::settings::{AppSettings, SettingsEvent};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn release_routes_are_explicit_and_stable_bypasses_github_listing() {
    let stable = release_route(false, "win-x64");
    assert_eq!(stable.channel, "win-x64-stable");
    assert_eq!(stable.source, ReleaseSource::StableHttp);
    let nightly = release_route(true, "osx-universal");
    assert_eq!(nightly.channel, "osx-universal-nightly");
    assert_eq!(nightly.source, ReleaseSource::NightlyGithub);
    assert_eq!(
        release_route(false, "osx-universal").channel,
        "osx-universal-stable"
    );
    assert_eq!(release_route(true, "win-x64").channel, "win-x64-nightly");
}

struct CheckRequest {
    nightly: bool,
    network: bool,
    reply: std::sync::mpsc::Sender<Result<Checked, BackendError>>,
}
struct FakeBackend(tokio::sync::mpsc::UnboundedSender<CheckRequest>);
impl Backend for FakeBackend {
    fn check(&self, nightly: bool, network: bool) -> Result<Checked, BackendError> {
        let (reply, response) = std::sync::mpsc::channel();
        self.0
            .send(CheckRequest {
                nightly,
                network,
                reply,
            })
            .unwrap();
        response.recv().unwrap()
    }
}
struct FakeCandidate {
    downloads: tokio::sync::mpsc::UnboundedSender<std::sync::mpsc::Sender<()>>,
    launches: Arc<AtomicUsize>,
}
impl Candidate for FakeCandidate {
    fn version(&self) -> String {
        "2026.601.1200".into()
    }
    fn download(&self) -> Result<UpdateInstallPlan, String> {
        let (reply, response) = std::sync::mpsc::channel();
        self.downloads.send(reply).unwrap();
        response.recv().unwrap();
        let launches = self.launches.clone();
        Ok(UpdateInstallPlan {
            authority: None,
            launch: Arc::new(move || {
                launches.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        })
    }
}
fn settings(bus: &AppEventBus, automatic: bool, nightly: bool) {
    bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
        settings: AppSettings {
            automatically_check_for_updates: automatic,
            include_nightly_updates: nightly,
            ..Default::default()
        },
    }));
}
async fn status(bus: &AppEventBus, expected: UpdateStatus) {
    let mut state = bus.state();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = state.borrow_and_update();
            if snapshot.updates.status == expected
                && (expected != UpdateStatus::Idle || snapshot.updates.current_version.is_some())
            {
                return;
            }
            drop(snapshot);
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
async fn request(
    requests: &mut tokio::sync::mpsc::UnboundedReceiver<CheckRequest>,
) -> CheckRequest {
    tokio::time::timeout(Duration::from_secs(3), requests.recv())
        .await
        .unwrap()
        .unwrap()
}
fn checked(candidate: Option<Arc<dyn Candidate>>) -> Checked {
    Checked {
        current: "2026.601.1100".into(),
        candidate,
    }
}

#[tokio::test]
async fn disabled_automatic_checks_still_allow_manual_check_and_download_never_installs() {
    let bus = AppEventBus::default();
    settings(&bus, false, false);
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    let probe = request(&mut requests).await;
    assert!(!probe.network);
    probe.reply.send(Ok(checked(None))).ok().unwrap();
    status(&bus, UpdateStatus::Idle).await;
    assert!(requests.try_recv().is_err());
    let (reply, response) = oneshot::channel();
    handle.send(UpdatesCommand::Check { reply }).await.unwrap();
    let check = request(&mut requests).await;
    assert!(check.network);
    assert!(!check.nightly);
    let (downloads, mut download_requests) = tokio::sync::mpsc::unbounded_channel();
    let launches = Arc::new(AtomicUsize::new(0));
    check
        .reply
        .send(Ok(checked(Some(Arc::new(FakeCandidate {
            downloads,
            launches: launches.clone(),
        })))))
        .ok()
        .unwrap();
    response.await.unwrap().unwrap();
    status(&bus, UpdateStatus::Available).await;
    assert!(download_requests.try_recv().is_err());
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::Download { reply })
        .await
        .unwrap();
    download_requests.recv().await.unwrap().send(()).unwrap();
    response.await.unwrap().unwrap();
    status(&bus, UpdateStatus::Ready).await;
    assert_eq!(launches.load(Ordering::SeqCst), 0);
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    // Canceling the dirty-session guard drops only this prepared copy.
    drop(response.await.unwrap().unwrap());
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    let plan = response.await.unwrap().unwrap();
    assert_eq!(launches.load(Ordering::SeqCst), 0);
    let queue = UpdateInstallQueue::default();
    queue.arm(plan).unwrap();
    assert_eq!(launches.load(Ordering::SeqCst), 0);
    let plan = queue.take().unwrap();
    assert!(queue.take().is_none());
    drop(plan);
    assert_eq!(launches.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn channel_change_rejects_pending_check_and_discards_late_completion() {
    let bus = AppEventBus::default();
    settings(&bus, false, false);
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    request(&mut requests)
        .await
        .reply
        .send(Ok(checked(None)))
        .ok()
        .unwrap();
    status(&bus, UpdateStatus::Idle).await;
    let (reply, response) = oneshot::channel();
    handle.send(UpdatesCommand::Check { reply }).await.unwrap();
    let stale = request(&mut requests).await;
    settings(&bus, false, true);
    assert!(response.await.unwrap().is_err());
    stale
        .reply
        .send(Err(BackendError::Failed("stale network failure".into())))
        .ok()
        .unwrap();
    status(&bus, UpdateStatus::Idle).await;
    assert_eq!(bus.state().borrow().updates.error, None);
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    assert!(response.await.unwrap().is_err());
}

#[tokio::test]
async fn channel_change_discards_download_and_staged_install_intent() {
    let bus = AppEventBus::default();
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    let check = request(&mut requests).await;
    assert!(check.network);
    let (downloads, mut download_requests) = tokio::sync::mpsc::unbounded_channel();
    let launches = Arc::new(AtomicUsize::new(0));
    check
        .reply
        .send(Ok(checked(Some(Arc::new(FakeCandidate {
            downloads,
            launches: launches.clone(),
        })))))
        .ok()
        .unwrap();
    status(&bus, UpdateStatus::Available).await;
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::Download { reply })
        .await
        .unwrap();
    let download = download_requests.recv().await.unwrap();
    settings(&bus, false, true);
    assert!(response.await.unwrap().is_err());
    download.send(()).unwrap();
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    assert!(response.await.unwrap().is_err());
    assert_eq!(launches.load(Ordering::SeqCst), 0);
    status(&bus, UpdateStatus::Idle).await;
}

#[tokio::test]
async fn unpackaged_application_projects_a_clear_unavailable_reason() {
    let bus = AppEventBus::default();
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (_handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    request(&mut requests)
        .await
        .reply
        .send(Err(BackendError::Unavailable(
            "Updates require an installed package.".into(),
        )))
        .ok()
        .unwrap();
    status(&bus, UpdateStatus::Unavailable).await;
    assert_eq!(
        bus.state().borrow().updates.error.as_deref(),
        Some("Updates require an installed package.")
    );
    assert_eq!(bus.state().borrow().updates.current_version, None);
}

#[tokio::test]
async fn automatic_checks_repeat_after_six_hours_but_stop_when_disabled() {
    let bus = AppEventBus::default();
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (_handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    let startup = request(&mut requests).await;
    assert!(startup.network);
    startup.reply.send(Ok(checked(None))).ok().unwrap();
    status(&bus, UpdateStatus::UpToDate).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(6 * 3600)).await;
    // Resume real time while crossing the blocking backend boundary.
    tokio::time::resume();
    let periodic = request(&mut requests).await;
    assert!(periodic.network);
    periodic.reply.send(Ok(checked(None))).ok().unwrap();
    status(&bus, UpdateStatus::UpToDate).await;
    settings(&bus, false, false);
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(6 * 3600)).await;
    tokio::time::resume();
    let (reply, response) = oneshot::channel();
    _handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    assert!(response.await.unwrap().is_err());
    assert!(requests.try_recv().is_err());
}

#[tokio::test]
async fn changing_channel_clears_an_already_downloaded_plan() {
    let bus = AppEventBus::default();
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    let (downloads, mut download_requests) = tokio::sync::mpsc::unbounded_channel();
    let launches = Arc::new(AtomicUsize::new(0));
    request(&mut requests)
        .await
        .reply
        .send(Ok(checked(Some(Arc::new(FakeCandidate {
            downloads,
            launches: launches.clone(),
        })))))
        .ok()
        .unwrap();
    status(&bus, UpdateStatus::Available).await;
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::Download { reply })
        .await
        .unwrap();
    download_requests.recv().await.unwrap().send(()).unwrap();
    response.await.unwrap().unwrap();
    status(&bus, UpdateStatus::Ready).await;
    settings(&bus, false, true);
    status(&bus, UpdateStatus::Idle).await;
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    assert!(response.await.unwrap().is_err());
    assert_eq!(bus.state().borrow().updates.available_version, None);
    assert_eq!(launches.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn network_failure_projects_installed_version_and_error() {
    let bus = AppEventBus::default();
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (_handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    request(&mut requests)
        .await
        .reply
        .send(Err(BackendError::InstalledFailure {
            reason: "Could not check for updates: network unavailable.".into(),
            current: "2026.601.1100".into(),
        }))
        .ok()
        .unwrap();
    status(&bus, UpdateStatus::Error).await;
    let state = bus.state();
    assert_eq!(
        state.borrow().updates.current_version.as_deref(),
        Some("2026.601.1100")
    );
    assert_eq!(
        state.borrow().updates.error.as_deref(),
        Some("Could not check for updates: network unavailable.")
    );
}

#[tokio::test]
async fn channel_change_rechecks_new_channel_after_stale_worker_drains() {
    let bus = AppEventBus::default();
    let mut events = bus.subscribe();
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (_handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    let stale = request(&mut requests).await;
    assert!(!stale.nightly);
    settings(&bus, true, true);
    // Observe owner invalidation before releasing the blocked old-channel worker.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let AppEvent::Updates(state) = events.recv().await.unwrap()
                && state.status == UpdateStatus::Idle
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    stale
        .reply
        .send(Err(BackendError::Failed("stale network failure".into())))
        .ok()
        .unwrap();
    let fresh = request(&mut requests).await;
    assert!(fresh.nightly);
    assert!(fresh.network);
    fresh.reply.send(Ok(checked(None))).ok().unwrap();
    status(&bus, UpdateStatus::UpToDate).await;
    while let Ok(event) = events.try_recv() {
        if let AppEvent::Updates(state) = event {
            assert_ne!(state.status, UpdateStatus::Error);
        }
    }
}

async fn ready_plan(
    bus: &AppEventBus,
) -> (
    UpdatesHandle,
    UpdateInstallPlan,
    Arc<AtomicUsize>,
    tokio::sync::mpsc::UnboundedReceiver<CheckRequest>,
) {
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    let (downloads, mut download_requests) = tokio::sync::mpsc::unbounded_channel();
    let launches = Arc::new(AtomicUsize::new(0));
    request(&mut requests)
        .await
        .reply
        .send(Ok(checked(Some(Arc::new(FakeCandidate {
            downloads,
            launches: launches.clone(),
        })))))
        .ok()
        .unwrap();
    status(bus, UpdateStatus::Available).await;
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::Download { reply })
        .await
        .unwrap();
    download_requests.recv().await.unwrap().send(()).unwrap();
    response.await.unwrap().unwrap();
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    let plan = response.await.unwrap().unwrap();
    (handle, plan, launches, requests)
}

#[tokio::test]
async fn issued_plan_rejects_channel_change_before_arm_and_never_revives() {
    let bus = AppEventBus::default();
    let (_handle, plan, launches, _requests) = ready_plan(&bus).await;
    let queue = UpdateInstallQueue::default();
    // Retention invalidates immediately, without waiting for actor watch delivery.
    settings(&bus, false, true);
    assert!(queue.arm(plan.clone()).is_err());
    assert!(queue.take().is_none());
    settings(&bus, false, false);
    assert!(queue.arm(plan.clone()).is_err());
    assert!(plan.launch_after_exit().is_err());
    assert_eq!(launches.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn armed_plan_revalidates_channel_before_launch() {
    let bus = AppEventBus::default();
    let (_handle, plan, launches, _requests) = ready_plan(&bus).await;
    let queue = UpdateInstallQueue::default();
    queue.arm(plan).unwrap();
    settings(&bus, false, true);
    assert!(queue.take().unwrap().launch_after_exit().is_err());
    assert_eq!(launches.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn actor_observed_channel_generation_invalidates_ticket_even_after_returning_to_old_channel()
{
    let bus = AppEventBus::default();
    let (_handle, plan, launches, _requests) = ready_plan(&bus).await;
    settings(&bus, false, true);
    status(&bus, UpdateStatus::Idle).await;
    settings(&bus, false, false);
    assert!(UpdateInstallQueue::default().arm(plan.clone()).is_err());
    assert!(plan.launch_after_exit().is_err());
    assert_eq!(launches.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn periodic_same_version_check_preserves_verified_download_for_retry() {
    let bus = AppEventBus::default();
    let (handle, _plan, launches, mut requests) = ready_plan(&bus).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(6 * 3600)).await;
    tokio::time::resume();
    let refresh = request(&mut requests).await;
    let (downloads, mut download_requests) = tokio::sync::mpsc::unbounded_channel();
    refresh
        .reply
        .send(Ok(checked(Some(Arc::new(FakeCandidate {
            downloads,
            launches: launches.clone(),
        })))))
        .ok()
        .unwrap();
    status(&bus, UpdateStatus::Ready).await;
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    let plan = response.await.unwrap().unwrap();
    assert!(download_requests.try_recv().is_err());
    assert_eq!(launches.load(Ordering::SeqCst), 0);
    let queue = UpdateInstallQueue::default();
    queue.arm(plan).unwrap();
    assert!(queue.take().is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn coalesced_channel_toggles_invalidate_issued_plan_before_actor_notification() {
    let bus = AppEventBus::default();
    let (handle, plan, launches, _requests) = ready_plan(&bus).await;
    // Do not yield: the Updates owner cannot observe the intermediate policy.
    settings(&bus, false, true);
    settings(&bus, false, false);
    let queue = UpdateInstallQueue::default();
    assert!(queue.arm(plan.clone()).is_err());
    assert!(queue.take().is_none());
    assert!(plan.launch_after_exit().is_err());
    let (reply, response) = oneshot::channel();
    handle
        .send(UpdatesCommand::PrepareInstall { reply })
        .await
        .unwrap();
    assert!(response.await.unwrap().is_err());
    status(&bus, UpdateStatus::Idle).await;
    assert_eq!(launches.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn coalesced_channel_toggles_reject_old_worker_completion() {
    let bus = AppEventBus::default();
    let mut events = bus.subscribe();
    let (checks, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let (_handle, task) = actor::build(bus.clone(), Arc::new(FakeBackend(checks)));
    task.spawn();
    let stale = request(&mut requests).await;
    settings(&bus, true, true);
    settings(&bus, true, false);
    stale.reply.send(Ok(checked(None))).ok().unwrap();
    // Starting the replacement check proves the stale worker has drained.
    let fresh = request(&mut requests).await;
    assert!(!fresh.nightly);
    while let Ok(event) = events.try_recv() {
        if let AppEvent::Updates(state) = event {
            assert_ne!(state.status, UpdateStatus::UpToDate);
            assert_eq!(state.current_version, None);
        }
    }
    fresh.reply.send(Ok(checked(None))).ok().unwrap();
    status(&bus, UpdateStatus::UpToDate).await;
}

#[test]
fn retained_channel_revision_tracks_every_toggle_without_dirtying_session() {
    let bus = AppEventBus::default();
    settings(&bus, false, false);
    assert_eq!(bus.state().borrow().update_channel_revision, 0);
    settings(&bus, true, false);
    assert_eq!(bus.state().borrow().update_channel_revision, 0);
    settings(&bus, true, true);
    assert_eq!(bus.state().borrow().update_channel_revision, 1);
    settings(&bus, true, true);
    assert_eq!(bus.state().borrow().update_channel_revision, 1);
    settings(&bus, true, false);
    assert_eq!(bus.state().borrow().update_channel_revision, 2);
    assert_eq!(bus.state().borrow().session_revision, 0);
    assert_eq!(bus.persisted_session_revision(), 0);
}

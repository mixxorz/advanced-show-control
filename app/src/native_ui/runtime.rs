use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};

use crate::application::ApplicationCommandContext;
use crate::lifecycle::AppLifecycle;
use crate::logging::{self, LoggingRuntime};
use crate::projector::ProjectionSubscription;
use crate::runtime::events::AppEventBus;
use crate::settings::build_settings_actor;
use crate::show::build_show_actor;
use crate::updates::{UpdateInstallQueue, build_updates_actor};

pub const APP_IDENTIFIER: &str = "com.advancedshowcontrol.app";

pub struct NativeRuntime {
    runtime: Arc<tokio::runtime::Runtime>,
    commands: ApplicationCommandContext,
    lifecycle: AppLifecycle,
    projections: Option<ProjectionSubscription>,
    _logging: LoggingRuntime,
    update_install_queue: UpdateInstallQueue,
    #[cfg(feature = "debug-tools")]
    debug_commands: crate::debug_tools::DebugRuntimeCommands,
}

impl NativeRuntime {
    /// @cc [owner:mixxorz,label:architecture] production-runtime-wiring
    /// Construction MUST create one Tokio runtime, one shared event bus, one app-lifetime Show and
    /// Settings and Updates owner, one Lifecycle using those owners, and exactly one projector
    /// subscription.
    pub fn build() -> Result<Self> {
        Self::build_at(app_config_dir())
    }

    #[cfg(feature = "debug-tools")]
    pub fn build_in(app_config_dir: PathBuf) -> Result<Self> {
        Self::build_at(app_config_dir)
    }

    fn build_at(app_config_dir: PathBuf) -> Result<Self> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("asc-runtime")
                .build()
                .context("failed to create the application runtime")?,
        );
        std::fs::create_dir_all(&app_config_dir).with_context(|| {
            format!(
                "failed to create application configuration directory {}",
                app_config_dir.display()
            )
        })?;
        #[cfg(feature = "debug-tools")]
        let debug_commands;
        let update_install_queue = UpdateInstallQueue::default();
        let (logging, commands, lifecycle) = {
            let _entered = runtime.enter();
            let event_bus = AppEventBus::default();
            let logging = logging::init_logging(&app_config_dir)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            logging.spawn_settings_watcher(event_bus.subscribe());
            let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
            let (settings, settings_task, initial_settings) =
                build_settings_actor(app_config_dir, event_bus.clone());
            logging.apply_settings(&initial_settings);
            let (updates, updates_task) = build_updates_actor(event_bus.clone());
            let lifecycle = AppLifecycle::new(
                event_bus,
                show.clone(),
                show_peers,
                lockout,
                settings.clone(),
            );
            show_task.spawn();
            settings_task.spawn();
            updates_task.spawn();
            #[cfg(feature = "debug-tools")]
            {
                debug_commands = crate::debug_tools::DebugRuntimeCommands::new(lifecycle.clone());
            }
            let commands = ApplicationCommandContext::new(
                lifecycle.clone(),
                show,
                settings,
                logging.ui_logs.clone(),
            )
            .with_updates(updates, update_install_queue.clone());
            (logging, commands, lifecycle)
        };
        let projections = runtime
            .block_on(commands.frontend_ready())
            .map_err(anyhow::Error::msg)
            .context("failed to start the application projector")?;

        Ok(Self {
            runtime,
            commands,
            lifecycle,
            projections: Some(projections),
            _logging: logging,
            update_install_queue,
            #[cfg(feature = "debug-tools")]
            debug_commands,
        })
    }

    pub fn handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    pub fn commands(&self) -> ApplicationCommandContext {
        self.commands.clone()
    }

    #[cfg(feature = "debug-tools")]
    pub fn debug_commands(&self) -> crate::debug_tools::DebugRuntimeCommands {
        self.debug_commands.clone()
    }

    pub fn take_projections(&mut self) -> ProjectionSubscription {
        self.projections
            .take()
            .expect("projection subscription may only be installed once")
    }

    /// @cc [owner:mixxorz,label:safety] native-shutdown-invalidates-runtime
    /// Native shutdown MUST request lifecycle disconnect before the Tokio runtime is dropped. If
    /// disconnect fails, shutdown MUST still force-invalidate the active generation and remove its
    /// generation-scoped peers, report the failure, and return `false`.
    pub fn shutdown(self) -> bool {
        let disconnect_result = self
            .runtime
            .block_on(self.commands.disconnect_lv1())
            .map(|_| ());
        self.runtime
            .block_on(finish_shutdown(&self.lifecycle, disconnect_result))
    }

    /// @cc [owner:mixxorz,label:safety] authorized-update-handoff-only
    /// Only a plan armed by the native dirty-session and disconnect continuation MAY launch an
    /// installer at exit. Ordinary Quit MUST NOT install a downloaded update. The helper MUST
    /// wait for the current process to exit before applying the update.
    pub fn launch_update_on_exit(&self) -> Result<(), String> {
        if let Some(plan) = self.update_install_queue.take() {
            plan.launch_after_exit().inspect_err(|error| {
                tracing::error!(
                    event = "update_install_handoff_failed",
                    "The update could not be started: {error}"
                );
            })?;
        }
        Ok(())
    }
}

async fn finish_shutdown(lifecycle: &AppLifecycle, disconnect_result: Result<(), String>) -> bool {
    if let Err(error) = disconnect_result {
        lifecycle.abort_current_runtime().await;
        tracing::warn!(
            event = "native_shutdown_disconnect_failed",
            error = %error,
            "Application shutdown could not finish disconnect cleanup: {error}"
        );
        return false;
    }
    true
}

pub fn app_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_IDENTIFIER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_path_retains_the_existing_application_identity() {
        let path = app_config_dir();

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(APP_IDENTIFIER)
        );
    }

    #[tokio::test]
    async fn native_shutdown_force_invalidates_after_disconnect_failure() {
        let lifecycle = AppLifecycle::default();
        let generation = lifecycle.active_generation().await;

        assert!(!finish_shutdown(&lifecycle, Err("show unavailable".to_string())).await);
        assert_eq!(lifecycle.active_generation().await, generation + 1);
    }
}

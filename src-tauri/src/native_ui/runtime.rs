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

pub const APP_IDENTIFIER: &str = "com.advancedshowcontrol.app";

pub struct NativeRuntime {
    runtime: Arc<tokio::runtime::Runtime>,
    commands: ApplicationCommandContext,
    projections: Option<ProjectionSubscription>,
    _logging: LoggingRuntime,
}

impl NativeRuntime {
    /// @cc [owner:mixxorz,label:architecture] production-runtime-wiring
    /// Construction MUST create one Tokio runtime, one shared event bus, one app-lifetime Show and
    /// Settings owner, one Lifecycle using those owners, and exactly one projector subscription.
    pub fn build() -> Result<Self> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("asc-runtime")
                .build()
                .context("failed to create the application runtime")?,
        );
        let app_config_dir = app_config_dir();
        std::fs::create_dir_all(&app_config_dir).with_context(|| {
            format!(
                "failed to create application configuration directory {}",
                app_config_dir.display()
            )
        })?;
        let (logging, commands) = {
            let _entered = runtime.enter();
            let event_bus = AppEventBus::default();
            let logging = logging::init_logging(&app_config_dir)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            logging.spawn_settings_watcher(event_bus.subscribe());
            let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
            let (settings, settings_task, initial_settings) =
                build_settings_actor(app_config_dir, event_bus.clone());
            logging.apply_settings(&initial_settings);
            let lifecycle = AppLifecycle::new(
                event_bus,
                show.clone(),
                show_peers,
                lockout,
                settings.clone(),
                initial_settings,
            );
            show_task.spawn();
            settings_task.spawn();
            let commands =
                ApplicationCommandContext::new(lifecycle, show, settings, logging.ui_logs.clone());
            (logging, commands)
        };
        let projections = runtime
            .block_on(commands.frontend_ready())
            .map_err(anyhow::Error::msg)
            .context("failed to start the application projector")?;

        Ok(Self {
            runtime,
            commands,
            projections: Some(projections),
            _logging: logging,
        })
    }

    pub fn handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    pub fn commands(&self) -> ApplicationCommandContext {
        self.commands.clone()
    }

    pub fn take_projections(&mut self) -> ProjectionSubscription {
        self.projections
            .take()
            .expect("projection subscription may only be installed once")
    }

    /// @cc [owner:mixxorz,label:safety] native-shutdown-invalidates-runtime
    /// Native shutdown MUST request lifecycle disconnect before the Tokio runtime is dropped so the
    /// active generation advances and generation-scoped peers are removed.
    pub fn shutdown(self) {
        if let Err(error) = self.runtime.block_on(self.commands.disconnect_lv1()) {
            tracing::warn!(
                event = "native_shutdown_disconnect_failed",
                error = %error,
                "Application shutdown could not finish disconnect cleanup: {error}"
            );
        }
    }
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
}

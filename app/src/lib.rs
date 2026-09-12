pub mod application;
mod atomic_file;
pub mod connection_state;
pub mod cue_lists;
#[cfg(feature = "debug-tools")]
pub mod debug_tools;
pub mod diagnostics;
pub mod fade;
pub mod lifecycle;
pub mod logging;
pub mod lv1;
pub mod native_ui;
pub mod projector;
pub mod runtime;
pub mod scenes;
pub mod session;
pub mod settings;
pub mod show;
pub mod show_file;
#[cfg(test)]
pub(crate) mod test_support;
pub mod time;

/// @cc [owner:mixxorz,label:architecture;testing] vegas-dev-tool-api
/// The deterministic Vegas helpers MUST remain exported from the library crate so the separate
/// development-tools crate can drive the same measured fader behavior without duplicating it.
pub mod vegas;

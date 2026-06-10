#[cfg(test)]
mod capture_tests;
mod events;
#[cfg(test)]
mod events_tests;
mod logs;
mod projection;
mod shell;
mod show_file_mapping;
#[cfg(test)]
mod show_file_mapping_tests;
#[cfg(test)]
mod test_support;
mod view;

#[allow(unused_imports)]
pub use shell::RuntimeHandles;
pub use shell::ShellState;
pub use view::{AppConnectionState, AppViewState};

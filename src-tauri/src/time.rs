use std::time::{SystemTime, UNIX_EPOCH};

/// @cc [owner:mixxorz,label:time] wall-clock-presentation-only
/// This function MUST return Unix-epoch milliseconds as a decimal string, using `0` when the system
/// clock predates the epoch. It MAY supply presentation or diagnostic timestamps, persisted
/// wall-clock labels, and collision-resistant filename prefixes; elapsed-time, deadline, and
/// authoritative ordering logic MUST use an explicit monotonic or injected time.
pub(crate) fn current_timestamp_millis() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

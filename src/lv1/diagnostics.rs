pub(crate) fn log_osc_rx(address: &str) {
    tracing::debug!(
        event = "osc_message",
        direction = "rx",
        osc_address = address,
        "OSC RX {address}"
    );
}

pub(crate) fn log_osc_tx(address: &str) {
    tracing::debug!(
        event = "osc_message",
        direction = "tx",
        osc_address = address,
        "OSC TX {address}"
    );
}

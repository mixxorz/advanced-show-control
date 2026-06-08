# Ideas

- Tighten Rust data models to make invalid states unrepresentable where practical. For example, stored scene fade targets should not allow a scoped channel without a valid stored fader value unless there is a deliberate migration or validation boundary.
- If hardware testing shows one exact final channel send is insufficient, add target verification with safe retry and visible mismatch reporting after fade completion.
- Improve startup and connection UX. The app should open on the connection screen, let the user choose from auto-discovered LV1 systems, remember the last connected system in a config file, auto-connect on launch when that system is available, and return to the connection screen whenever LV1 is disconnected.
- Standardize module type naming and placement. Some modules use `types.rs` while others use `model.rs`; pick one convention for domain data structures and apply it consistently unless a module has a clear reason to differ.
- Add an event trigger/action automation engine. Users should be able to define automations with triggers and actions, so incoming events can drive configured app behavior without hard-coding every workflow into a dedicated runtime task.
- Track LV1 scene order changes and renames for the current active show, and consider a reconciliation flow when loading a show file whose stored scene references no longer match the current LV1 scene list.

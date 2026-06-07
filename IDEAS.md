# Ideas

- Tighten Rust data models to make invalid states unrepresentable where practical. For example, stored scene fade targets should not allow a scoped channel without a valid stored fader value unless there is a deliberate migration or validation boundary.
- Consider copying Avid VENUE snapshot crossfade behavior for advanced overlap: when a new scene is recalled during an active fade, scoped faders in the incoming scene should take over from their current mid-fade value and move to the incoming scene's stored value using the incoming scene's crossfade time, while active fades for faders not scoped in the incoming scene continue. Non-faded parameters still update immediately on each recall.

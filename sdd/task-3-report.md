Status: DONE

Summary of changes:
- Added cue-list persistence to `.ascs` schema v2 and preserved cue-list documents on import/export.
- Wired show lifecycle save/load/new flows to read and replace cue-list state through the cue-list actor.
- Projected cue-list state into `AppViewState` and the projector cache, and handled cue-list events in the projector runtime.
- Added show-actor dirty-state handling for persisted cue-list edits.

Tests run:
- `cargo nextest run -p advanced-show-control show::show_file projector::cache projector::runtime show::actor lifecycle` - passed
- `cargo fmt --all` - passed
- `cargo clippy --workspace --all-targets -- -D warnings` - passed

Commits created:
- `641ce7a` `feat: persist and project cue lists`

Self-review notes and concerns:
- `import_show_file` still accepts schema v1 and drops legacy cue-list data, which matches the task brief.
- Scene-level cue state remains in place for Task 4 to remove later.

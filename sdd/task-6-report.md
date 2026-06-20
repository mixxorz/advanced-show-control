# Task 6 Report

## Commands

- `cargo nextest run -p advanced-show-control connect_initial_lv1_snapshot_uses_lifecycle_command_bus_targets reconnect_timed_out_aborts_runtime_and_clears_command_bus_for_matching_attempt connect_installs_runtime_targets_before_scene_recall_startup`
- `cargo nextest run -p advanced-show-control commands::tests lifecycle show::commands`
- `cargo fmt --all -- --check`
- `cargo clippy -p advanced-show-control --all-targets -- -D warnings`

## Results

- Targeted connect and lifecycle tests passed.
- Required test suites passed: 101 passed, 417 skipped.
- Formatting check passed.
- Clippy passed.

## Commit

- `2b477f1` `fix: query initial lv1 state through command bus`

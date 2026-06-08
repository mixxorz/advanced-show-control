# Rust Tooling Design

## Goal

Add local Rust quality tooling for the workspace:

- Use `cargo-nextest` as the preferred Rust test runner.
- Add pre-commit hooks for Rust formatting and clippy.
- Keep the setup minimal and workspace-wide so both Rust crates are checked consistently.

## Selected Approach

Use workspace-wide local pre-commit hooks:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

Do not run tests in pre-commit. Tests remain an explicit developer command, using `cargo nextest run --workspace` for normal full Rust verification.

## Files

- `.pre-commit-config.yaml` defines local hooks for rustfmt and clippy.
- `rust-toolchain.toml` pins the project to the stable Rust toolchain and ensures rustfmt and clippy are installed as toolchain components.
- `.config/nextest.toml` stores nextest profile configuration, including a 10-second slow-test timeout period.
- `README.md` records developer setup commands for pre-commit, clippy, rustfmt, and nextest.
- `AGENTS.md` records the preferred agent verification commands so future automation uses the same tooling.

## Behavior

The format hook fails commits when Rust formatting is not current. Developers fix this with `cargo fmt --all`.

The clippy hook checks every workspace target and treats warnings as errors. This catches issues across both the core crate and the Tauri shell crate, including tests and examples if present.

Nextest is configured but not forced in pre-commit. This avoids making every commit depend on the full test suite while still giving the project a clear default Rust test runner. Tests are considered slow after 10 seconds so unexpectedly long-running tests are visible during local verification.

Documentation is updated in both human-facing and agent-facing locations. `README.md` explains how a developer installs and runs the tools locally. `AGENTS.md` updates the verification guidance used by automation in this workspace.

The Rust toolchain file uses the stable channel rather than a fixed compiler version. This keeps edition 2024 support and standard components consistent without requiring manual version bumps for every Rust release.

## Tradeoffs

Workspace-wide hooks are slower than changed-file hooks, but they avoid partial checks that can miss cross-crate or test-target warnings. That fits this project's safety-sensitive fader-control behavior.

Keeping tests out of pre-commit avoids overly slow local commits. Broader verification still uses `cargo nextest run --workspace` before claiming code changes are complete.

## Verification

After implementation, verify with:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --workspace`

If nextest is not installed, install it with `cargo install cargo-nextest --locked` before running the nextest command.

# Rust Tooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add workspace-wide Rust formatting, clippy, nextest, and pre-commit tooling.

**Architecture:** This is a configuration-only change. `rust-toolchain.toml` standardizes required Rust components, `.pre-commit-config.yaml` runs workspace-wide format and clippy checks, `.config/nextest.toml` records the default nextest profile and slow-test timeout, and docs explain how humans and agents should use the tooling.

**Tech Stack:** Rust 2024, rustup, rustfmt, clippy, cargo-nextest, pre-commit.

---

## File Structure

- Create: `rust-toolchain.toml` for stable Rust plus `rustfmt` and `clippy` components.
- Create: `.pre-commit-config.yaml` for local workspace-wide Rust hooks.
- Create: `.config/nextest.toml` for the default nextest profile.
- Modify: `README.md` with developer setup and command usage.
- Modify: `AGENTS.md` with updated verification commands.

### Task 1: Toolchain And Hook Config

**Files:**
- Create: `rust-toolchain.toml`
- Create: `.pre-commit-config.yaml`

- [ ] **Step 1: Add the Rust toolchain file**

Create `rust-toolchain.toml` with:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 2: Add pre-commit hooks**

Create `.pre-commit-config.yaml` with:

```yaml
repos:
  - repo: local
    hooks:
      - id: cargo-fmt
        name: cargo fmt
        entry: cargo fmt --all -- --check
        language: system
        pass_filenames: false
        types: [rust]

      - id: cargo-clippy
        name: cargo clippy
        entry: cargo clippy --workspace --all-targets -- -D warnings
        language: system
        pass_filenames: false
        types: [rust]
```

- [ ] **Step 3: Verify formatting hook command**

Run: `cargo fmt --all -- --check`

Expected: command exits successfully. If it fails because files need formatting, run `cargo fmt --all`, then rerun `cargo fmt --all -- --check`.

- [ ] **Step 4: Verify clippy hook command**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: command exits successfully with no warnings.

### Task 2: Nextest Config

**Files:**
- Create: `.config/nextest.toml`

- [ ] **Step 1: Add the nextest config directory**

Ensure `.config/` exists.

- [ ] **Step 2: Add nextest profile configuration**

Create `.config/nextest.toml` with:

```toml
[profile.default]
retries = 0
fail-fast = false
slow-timeout = { period = "10s", terminate-after = 2 }
```

- [ ] **Step 3: Verify nextest configuration**

Run: `cargo nextest run --workspace`

Expected: all Rust tests pass. If the command is unavailable, install nextest with `cargo install cargo-nextest --locked`, then rerun `cargo nextest run --workspace`.

### Task 3: Human Documentation

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Locate the developer setup section**

Open `README.md` and find the existing setup, test, or development section. If no such section exists, add a `## Development Tooling` section near other setup information.

- [ ] **Step 2: Document the tooling setup**

Add this content, adapting only the heading level if needed to match the surrounding README structure:

```markdown
## Development Tooling

This workspace uses `rust-toolchain.toml` to select stable Rust with `rustfmt` and `clippy` installed.

Install local developer tools:

```bash
cargo install cargo-nextest --locked
pre-commit install
```

Run Rust checks manually:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
```

The pre-commit hooks run Rust formatting and clippy workspace-wide. Tests are not run in pre-commit; use `cargo nextest run --workspace` for full Rust test verification.
```

- [ ] **Step 3: Check Markdown formatting**

Read the edited `README.md` section and confirm fenced code blocks are balanced.

### Task 4: Agent Documentation

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: Update common Rust checks**

Replace the common Rust checks block with:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo build --workspace
```

- [ ] **Step 2: Update targeted check guidance**

Keep existing targeted `cargo test` commands for narrow development loops unless they are already duplicated elsewhere. Add one sentence after the common Rust checks block:

```markdown
Use `cargo test ...` for targeted inner-loop checks when nextest would be broader than needed.
```

- [ ] **Step 3: Check agent guidance consistency**

Read the `Verification Commands` section and confirm it mentions nextest for full Rust tests and keeps targeted commands available.

### Task 5: Final Verification

**Files:**
- Verify all changed files.

- [ ] **Step 1: Run Rust formatting verification**

Run: `cargo fmt --all -- --check`

Expected: command exits successfully.

- [ ] **Step 2: Run clippy verification**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: command exits successfully with no warnings.

- [ ] **Step 3: Run nextest verification**

Run: `cargo nextest run --workspace`

Expected: all Rust tests pass.

- [ ] **Step 4: Run pre-commit verification**

Run: `pre-commit run --all-files`

Expected: both local hooks pass. If pre-commit is unavailable, install it with the user's preferred Python/package-manager workflow and rerun, or report that verification was blocked by a missing `pre-commit` executable.

- [ ] **Step 5: Inspect the diff**

Run: `git diff -- rust-toolchain.toml .pre-commit-config.yaml .config/nextest.toml README.md AGENTS.md docs/superpowers/specs/2026-06-09-rust-tooling-design.md docs/superpowers/plans/2026-06-09-rust-tooling.md`

Expected: diff contains only the intended tooling and documentation changes.

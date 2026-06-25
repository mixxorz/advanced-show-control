# Coding Conventions

This document is the source of truth for day-to-day implementation conventions in this repository. `docs/architecture.md` defines system boundaries and runtime architecture. This file defines how code should be written inside those boundaries.

## General Principles

- Prefer the smallest correct change.
- Keep code paths explicit and easy to reason about. This app controls live mixer faders.
- Follow existing module patterns unless the task is specifically to improve structure.
- Do not add backward-compatibility code unless persisted data, shipped behavior, external consumers, or an explicit requirement need it.
- Keep docs current when behavior, architecture, or project phase changes.
- Add future ideas to the appropriate release section in `docs/roadmap.md` instead of expanding current scope.

## Rust Backend

- Core backend code lives under `src-tauri/src/` in the `advanced-show-control` crate.
- Domain state belongs to the actor or module that owns that domain.
- Actor handles must remain dumb cloneable mailbox senders. Do not add convenience methods that hide command enum construction.
- Callers should construct command enum variants explicitly and attach a `oneshot` reply when they need a result.
- Tauri command adapters must stay thin: deserialize frontend input, send actor commands, await replies, and map errors into frontend-safe strings.
- Business logic belongs in owning actors/modules, not in Tauri command adapters or actor handles.
- Import public domain items from the module root. Do not import from private submodules unless the module intentionally exposes that path.

## Logging

- Use `tracing` as the application logging API.
- Every application log should include a stable `event` field and a complete human-readable message.
- User-facing log messages must be understandable without reading structured fields.
- Structured fields are for diagnostics, filtering, and support logs. Do not rely on them as the only explanation.
- Do not dump full settings files, show files, OSC payloads, or other noisy/sensitive state into user-facing logs.
- Do not duplicate the same fact at multiple layers. Log the request or low-level mechanics at `DEBUG`; log the resulting user-visible fact at `INFO`, `WARN`, or `ERROR` as appropriate.

### Logging Levels

- `DEBUG`: protocol details, internal decisions, noisy diagnostics, subscriber lag details, state counts, low-level write/drop information, no-op operations, and actor shutdown details. `DEBUG` events go to diagnostic file/stdout sinks, not the frontend log UI.
- `INFO`: user-relevant operational facts and successful state changes that an engineer may need to understand app behavior, such as connection progress, scene/cue actions, completed user-requested file operations, settings updates, and non-noisy reconciliation outcomes.
- `WARN`: visible safety blocks, skipped/blocked operations, recoverable failures, invalid user-owned files that fall back to safe defaults, and user-relevant conditions that need attention.
- `ERROR`: command failures, unrecoverable runtime setup failures, and failed file writes that prevent diagnostics or user-requested persistence.

### Logging Delivery

- Runtime modules emit tracing events only. They do not publish `AppEventBus` events solely to create logs.
- `DEBUG` and above are written to diagnostic logs.
- `INFO`, `WARN`, and `ERROR` are projected into frontend log state through the tracing UI sink.
- The frontend receives log state only through `app-status-changed` snapshots. It must not subscribe directly to backend logs.

## Rust Tests

Rust tests should fit one of these categories:

- Pure unit tests that call functions directly and have no side effects.
- Actor tests that interact through the actor mailbox, `AppEventBus`, and a tracing listener when tracing output is part of behavior under test.
- Smoke tests through the debug module/app.

Do not test side-effecting actor behavior by directly mutating actor internals or inspecting private state.

Use `cargo nextest run ...` for Rust tests, including targeted inner-loop checks. Avoid `cargo test` unless a test harness feature specifically requires it.

## Frontend

- Frontend code lives under `ui/`; do not assume a root `src/` frontend.
- Preserve the existing design language unless the task is to redesign it.
- Define reusable fonts, colors, spacing, borders, and interaction states as Tailwind/CSS theme variables when a value is reusable.
- Avoid hard-coded Tailwind values when a reusable token is appropriate.
- Keep frontend state projected from backend snapshots. Do not bypass `app-status-changed` for backend-owned state.
- Use full-object replacement for settings updates unless the backend API explicitly exposes a narrower command.

## Safety-Critical Code

- Do not bypass lockout checks.
- Do not bypass exact scene identity validation unless the task explicitly changes the matching model.
- Do not bypass generation guards. Stale tasks must not send fader commands or write misleading UI logs after disconnect or reconnect.
- Do not send fader commands when LV1 state is unavailable, disconnected, stale, or unsafe.
- Scene recall automation must validate before aborting an existing fade.
- Blocked, skipped, or disabled recalls must not abort an existing fade.
- Use fresh LV1 state for recall automation where event subscriber ordering could otherwise create stale decisions.
- Make safety blocks visible through logs, facts, or projected UI state.
- Preserve manual override, abort, overlap/same-scene, and disconnect safety behavior.

## Verification

- Use the smallest relevant `make` target or direct Cargo/npm command while developing.
- Before claiming work is complete, run the verification command that proves the claim and read the output.
- CI-style verification is `make check`.
- Hook-only checks should run at commit time; do not bypass hooks.

Common targeted checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
npm --prefix ui run format:check
npm --prefix ui run lint
npm --prefix ui run typecheck
npm --prefix ui run test
```

## Commits

- Commit early and often in this repo unless explicitly asked not to commit.
- Check `git status --short` before committing.
- Inspect the relevant `git diff` before committing.
- Stage only intended files.
- Do not include unrelated user or agent changes.
- Use concise commit messages that match the repo style, such as `feat: ...`, `fix: ...`, `test: ...`, or `docs: ...`.
- Run relevant verification before committing code changes.
- Do not amend commits unless explicitly asked.
- Do not force-push unless explicitly asked.
- Do not use destructive git commands such as `git reset --hard` or `git checkout -- <path>` unless explicitly asked.

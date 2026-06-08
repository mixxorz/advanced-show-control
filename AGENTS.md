# AGENTS.md

## Project Context

This project is a Tauri/Rust/React desktop app that adds timed fader fades to Waves eMotion LV1 and LV1 Classic scene workflows.

Project layout:

- `src/` contains the core Rust crate, `advanced-show-control`.
- `src-tauri/` contains the Tauri desktop shell crate, `advanced-show-control-tauri`, which depends on the core Rust crate.
- `ui/` contains the React/TypeScript frontend.

Do not assume `src/` is the frontend; this project does not use the default Tauri template layout.

LV1 remains the source of truth for scene creation and scene recall. The app is a fader-fade overlay. It stores fade metadata for LV1 scenes and moves only the scoped faders that the engineer has configured.

The app owns:

- Which LV1 scenes have app-managed fade behavior.
- Which faders are scoped into each app-managed scene.
- Stored target fader values for scoped faders.
- Fade duration, fade execution, and safety behavior.
- Show-file storage for the app's fade configuration.

LV1 owns:

- Scene creation and scene recall.
- Normal console state such as routing, plugins, mutes, processing, and non-app-managed scene scope.
- Live mixer state.

Current architecture is actor-oriented:

- `Lv1Actor` owns the LV1 TCP connection and mirrored LV1 state.
- `FadeEngine` owns active fade timing and fader writes.
- `ShowState` owns show data and show-file state.
- `SceneRecallFader` owns scene recall policy and starts validated scene fades.
- `ShellState` is the Tauri-side projection and command surface for UI state.
- `AppEventBus` broadcasts facts/events.
- `AppCommandBus` routes acknowledged commands to the current LV1 and fade targets.

Read these files before substantial work:

- `PROJECT.md` for product intent and safety model.
- `PHASES.md` for current project phase and deferred work.
- `docs/architecture.md` for runtime architecture.
- `IDEAS.md` for follow-up ideas that should not be silently implemented as part of unrelated work.

## Agent Developer Guidance

- Prefer the smallest correct change.
- Use TDD for behavior changes, bug fixes, and safety logic.
- Keep code paths explicit and easy to reason about; this project controls live mixer faders.
- Follow existing module patterns unless the task is specifically to clean up structure.
- Do not make broad refactors while implementing a feature unless they are required for the feature and covered by tests.
- Do not add backward-compatibility code unless there is a concrete need.
- Keep docs current when behavior, architecture, or project phase changes.
- Add future ideas to `IDEAS.md` instead of expanding the current scope.
- For UI work, preserve the existing design language unless the task is to redesign it.

## Safety-Critical Rules

- Do not bypass lockout checks.
- Do not bypass exact scene identity validation unless the task explicitly changes the matching model.
- Do not bypass generation guards. Stale tasks must not send fader commands or write misleading UI logs after disconnect or reconnect.
- Do not send fader commands when LV1 state is unavailable, disconnected, stale, or unsafe.
- Scene recall automation must validate before aborting an existing fade.
- Blocked, skipped, or disabled recalls must not abort an existing fade.
- Use fresh LV1 state for recall automation where event subscriber ordering could otherwise create stale decisions.
- Make safety blocks visible through logs or UI state.
- Preserve manual override, abort, finish-now, and disconnect safety behavior.

## Verification Commands

Use the smallest relevant command while developing, then run broader verification before completion.

Common Rust checks:

```bash
cargo test --workspace
cargo build --workspace
```

Common frontend checks:

```bash
npm run typecheck
npm run build
```

Useful targeted Rust checks:

```bash
cargo test -p advanced-show-control-tauri scene_recall
cargo test -p advanced-show-control-tauri commands::tests
cargo test -p advanced-show-control fade
```

Before claiming work is complete, run the verification command that proves the claim and read the output.

## Commit Rules

- Check `git status --short` before committing.
- Inspect the relevant `git diff` before committing.
- Stage only intended files.
- Do not include unrelated user or agent changes.
- Use concise commit messages that match the repo style, such as `feat: ...`, `fix: ...`, `test: ...`, or `docs: ...`.
- Run relevant verification before committing code changes.
- Do not amend commits unless explicitly asked.
- Do not force-push unless explicitly asked.
- Do not use destructive git commands such as `git reset --hard` or `git checkout -- <path>` unless explicitly asked.
- If tests or hooks fail, fix the issue in a new commit rather than hiding or bypassing the failure.

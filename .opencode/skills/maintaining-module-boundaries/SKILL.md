---
name: maintaining-module-boundaries
description: Use when changing actors, native command adapters, domain ownership, projection contracts, state flow, command routing, or module responsibilities in this LV1 fade utility.
---

# Maintaining Module Boundaries

## Overview

Put behavior where the owning domain can enforce it. Boundaries should make ownership, generation scope, and safety checks explicit.

## When To Use

- Changing actor state, mailbox commands, `AppEventBus` facts, or projected `AppViewState`.
- Adding or changing GPUI/native command adapters under `app/src/native_ui/`.
- Moving logic among native UI adapters, actors, projectors, or domain modules under `app/src/`.
- Changing settings, show, scenes, cue lists, fade, lifecycle, or LV1 state flow.
- Changing the non-production CLIs in `dev-tools/` that exercise production runtime commands.

## Required Context

- Read `docs/architecture.md` for current ownership, lifecycle, projection, persistence, and safety boundaries.
- Read `docs/coding-conventions.md` for Rust, native UI, logging, testing, and verification rules.
- Read `docs/lv1-osc.md` when changing LV1 protocol behavior.
- Before changing or reviewing production code, run `cc-check list path/to/file.rs[:line]` for affected files and called declarations. Include ancestor `CONTRACTS` obligations, and keep contract prose, implementation, and behavior tests aligned.

## Ownership Rules

- Production code lives in the `app/` crate. Domain actors and services live under `app/src/`; the GPUI Kit host and views live under `app/src/native_ui/`.
- `dev-tools/` is a separate, non-publishable crate for the hardware-smoke and LV1 probe CLIs. It must not be linked into the production binary.
- Domain state belongs to its owning actor or module. Follow the ownership table in `docs/architecture.md` rather than creating duplicate state.
- Actor handles remain dumb cloneable typed Tokio senders. Callers construct command enum variants explicitly and attach `oneshot` replies only when results are needed.
- Shared adapter helpers may own request/reply plumbing, but must not hide domain commands or policy.
- Business logic belongs in owning actors/modules, not native UI adapters or handles.
- Import public domain items from module roots unless a submodule intentionally exposes a public path.

## Boundary Rules

| Location | Responsibility |
|---|---|
| GPUI view/entity | Render projected state and own presentation-only state |
| Native command adapter | Translate intent, schedule Tokio work, dispatch explicit commands, and map UI-safe errors |
| Actor/domain module | Own state, validate behavior, enforce policy, and emit facts or tracing at the correct seam |
| `AppEventBus` | Broadcast facts and retain the latest app-lifetime projections; never carry requests |
| Projector | Build complete, versioned `AppViewState` snapshots and publish through the native projection sink |
| Lifecycle | Own connection generations and install/remove complete generation-scoped LV1/Fade peers |

## State and Safety Flow

- Keep domain-owned state projected through complete `AppViewState` snapshots; do not create alternate UI sources of truth.
- GPUI context types must not enter actors, and Tokio runtime types must not enter rendering.
- Handlers must not block the GPUI thread. Schedule asynchronous work and return accepted updates to GPUI.
- Preserve lockout, exact scene identity, fresh LV1 state, and generation guards.
- Never allow stale generations to send fader commands or publish misleading success/log state.
- Validate recall automation before Fade admission or aborting an existing fade; blocked, skipped, or disabled pre-admission recalls must not abort it.

## Common Mistakes

- Hiding actor commands behind handle convenience methods.
- Putting validation or business policy in GPUI handlers or native adapters.
- Treating `AppEventBus` as a request bus.
- Letting UI-local state duplicate domain state or overwrite a newer snapshot version.
- Retagging or partially installing generation-scoped peers.
- Importing through private submodules when the module root exposes the API.
- Refactoring broadly instead of making the smallest correct boundary change.

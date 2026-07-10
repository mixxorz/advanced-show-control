---
name: maintaining-module-boundaries
description: Use when changing actors, Tauri commands, backend ownership, frontend-backend contracts, state flow, command routing, or module responsibilities in this LV1 fade utility.
---

# Maintaining Module Boundaries

## Overview

Put behavior where the owning domain can enforce it. Boundaries should make safety and ownership obvious, not hide command flow behind convenience layers.

## When To Use

- Changing actor state, actor commands, command buses, event buses, or projected app state.
- Adding or changing Tauri commands.
- Moving logic between frontend, Tauri adapters, actors, or domain modules.
- Changing how settings, show state, fade state, scene recall, or LV1 state flows.

## Ownership Rules

- Domain state belongs to the actor or module that owns that domain.
- Actor handles stay dumb cloneable mailbox senders.
- Do not add actor-handle convenience methods that hide command enum construction.
- Callers construct command enum variants explicitly and attach `oneshot` replies when they need results.
- Business logic belongs in owning actors/modules, not Tauri command adapters or actor handles.
- Import public domain items from module roots unless a submodule intentionally exposes a path.

## Boundary Rules

| Location | Responsibility |
|---|---|
| Tauri command adapter | Deserialize frontend input, send actor command, await reply, map errors to frontend-safe strings |
| Actor/module | Own domain state, validate behavior, enforce policy, emit facts/logs at the proper seam |
| Frontend | Present projected state and collect user intent |
| Projector | Build `AppViewState` snapshots and emit `app-status-changed` |

## Frontend-Backend State Flow

- Keep backend-owned state projected from backend snapshots.
- Do not bypass `app-status-changed` for backend-owned state.
- Use full-object replacement for settings updates unless the backend API exposes a narrower command.
- Keep frontend logic thin; functionality and safety policy belong in the backend.

## Common Mistakes

- Hiding actor commands behind convenience methods because it feels cleaner.
- Putting validation or business rules in Tauri adapters.
- Letting frontend local state become an alternate source of truth for backend-owned data.
- Importing through private submodules when the module root exposes the intended API.
- Refactoring broadly instead of making the smallest correct boundary change.

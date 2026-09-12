---
name: building-ui
description: Use when changing the native GPUI Kit interface, styling, projected UI state, settings forms, visual tests, or user interaction flows in this LV1 fade utility.
---

# Building UI

## Overview

The native UI presents domain-owned state and captures user intent. Keep GPUI code focused on rendering, presentation state, and command dispatch. Domain validation and safety policy belong to the owning Rust actor or module.

## When To Use

- Changing the GPUI Kit host or views under `app/src/native_ui/`.
- Adding or changing controls, styling, forms, dialogs, menus, or user flows.
- Changing `AppViewState` projection or native visual/component tests.
- Displaying domain-owned state or dispatching commands from GPUI.

## Core Rules

- Production code lives in the `app/` crate; native UI code lives under `app/src/native_ui/`.
- Read `docs/architecture.md` and `docs/coding-conventions.md` before changing state flow or native UI boundaries.
- Before changing production code, discover applicable `@cc` comments and ancestor `CONTRACTS` files with `cc-check list path/to/file.rs[:line]`. Keep contracts, implementation, and tests aligned.
- Preserve the existing design language unless the task is a redesign.
- Define reusable fonts, colors, spacing, borders, and interaction states as GPUI theme tokens.
- Render domain-owned state from complete, versioned `AppViewState` snapshots. GPUI entities may own presentation-only state such as focus, open dialogs, drag previews, and pending text.
- Keep native command adapters thin. They may translate UI intent, schedule asynchronous work, dispatch explicit actor commands, and map errors for display; they must not own business or safety policy.
- Never block the GPUI thread on actor replies, network work, discovery, file I/O, or dialogs. Use the dedicated Tokio runtime and schedule accepted updates on the GPUI thread.
- Use full-object replacement for settings unless the domain API explicitly provides a narrower command.
- Do not add npm, React, TypeScript, browser-test, Storybook, or Tauri dependencies.

## Native Testing

- Add or update `#[gpui_kit::test]` component tests when UI behavior changes.
- Use `make visual-test` on macOS for reviewed native visual snapshots.
- Use `make visual-update` only after inspecting and accepting intentional visual changes.
- Run the smallest targeted `cargo nextest run ...` command during development; use `make check` for broad CI-style verification when warranted.
- Windows appearance requires manual visual acceptance where equivalent image rendering is unavailable.

## UI Boundary Guide

| Need | Preferred owner |
|---|---|
| Formatting projected values | GPUI view/helper |
| Focus, dialog, drag, or input-draft state | GPUI entity |
| Persisting settings or show data | Owning actor through an explicit command |
| Validating LV1, fade, or scene safety | Owning domain actor/module |
| Building complete versioned snapshots | Projector |
| Displaying disabled, blocked, or error state | GPUI from projected state or UI-safe command results |

## Common Mistakes

- Duplicating actor state machines in GPUI entities.
- Implementing safety decisions in event handlers or adapters.
- Blocking the GPUI thread while awaiting Tokio work.
- Applying an older snapshot over a newer `state_version`.
- Adding one-off visual values instead of reusable GPUI theme tokens.
- Changing visible states without updating native component or visual coverage.

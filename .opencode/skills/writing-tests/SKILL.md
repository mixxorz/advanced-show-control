---
name: writing-tests
description: Use when adding, changing, reviewing, or choosing Rust, actor, dev-tools smoke, or GPUI Kit native tests for this LV1 fade utility.
---

# Writing Tests

## Overview

Tests must prove behavior through public seams. This app controls live mixer faders, so test observable inputs, outputs, state changes, and side effects instead of private implementation details.

## When To Use

- Adding or changing a test.
- Fixing a bug or changing behavior that needs coverage.
- Choosing among pure unit, actor, hardware-smoke, GPUI Kit component, or native visual coverage.
- Reviewing tests for brittle coupling or missing safety behavior.

## Core Rules

- Start from the behavior that must remain true and use the smallest test boundary that proves it.
- Keep Rust behavior tests in the approved categories: pure unit tests, actor tests through actor mailboxes and `AppEventBus`, or smoke tests through the separate `dev-tools/` smoke CLI.
- Add a tracing listener to actor tests when tracing output is part of the behavior.
- Do not test side-effecting actors by mutating internals or inspecting private state.
- Do not write source-string tests that use `include_str!` or similar mechanisms to assert on implementation text.
- Test behavior through public functions, actor mailboxes, native command adapters, projected state, or the hardware-smoke CLI.
- Prefer targeted `cargo nextest run ...` commands while developing. Avoid `cargo test` unless a required harness feature is unavailable in nextest.
- Keep safety tests explicit about lockout, generation freshness, exact scene identity, stale or unavailable LV1 state, and whether an existing fade may be aborted.

## Rust Test Selection

| Behavior | Test style |
|---|---|
| Pure formatting, parsing, matching, interpolation, or policy with no side effects | Pure unit test that calls the function directly |
| Actor command, state transition, event, projection fact, or safety decision | Actor test through the mailbox and `AppEventBus` |
| User-facing tracing behavior | Actor test through public seams with a tracing listener |
| LV1-compatible production workflow or live fader behavior | Hardware smoke test through `dev-tools/` |

Production tests belong with the `advanced-show-control` crate under `app/`. Development CLI tests use the separate `dev-tools/Cargo.toml` manifest. Development-tool code must not be linked into the production binary.

## GPUI Kit Test Selection

GPUI Kit host and view code lives under `app/src/native_ui/`.

| Behavior | Test style |
|---|---|
| Native component rendering, interaction, disabled state, or projected-state handling | `#[gpui_kit::test]` component test |
| Native appearance and reviewed visual states on macOS | `make visual-test` |
| Intentional accepted visual change | Inspect the change, then run `make visual-update` |
| Windows appearance | Manual visual acceptance when equivalent image rendering is unavailable |

Keep domain validation and safety policy in actor/domain tests rather than duplicating those rules in GPUI tests. GPUI tests should cover rendering, presentation-only state, user intent, and native adapter behavior.

## Commands

```bash
cargo nextest run -p advanced-show-control <test-filter>
cargo nextest run --workspace
cargo nextest run --manifest-path dev-tools/Cargo.toml
make visual-test
make check
make smoke
```

`make check` runs formatting, linting, tests, and builds for the production and development-tool crates. It does not prove a specific native visual or hardware workflow.

`make smoke` requires an LV1-compatible environment. After it runs, inspect `logs/debug-smoke-report.txt`; the terminal output is not the authoritative suite result.

## Common Mistakes

- Testing actor internals instead of sending commands through the actor mailbox.
- Asserting on Rust source text instead of behavior.
- Testing a safety decision only in GPUI when it belongs to the owning domain actor.
- Covering only the happy path for safety-sensitive behavior.
- Treating `make check` as proof of native appearance or live LV1 behavior.
- Updating native visual snapshots without inspecting the intentional change.
- Using `cargo test` by habit when `cargo nextest run ...` is the project default.

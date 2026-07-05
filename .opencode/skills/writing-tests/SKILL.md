---
name: writing-tests
description: Use when adding, changing, reviewing, or choosing tests for this LV1 fade utility, including Rust actor tests, unit tests, smoke tests, Vitest, Storybook tests, or behavior coverage.
---

# Writing Tests

## Overview

Tests must prove behavior through public seams. This app controls live mixer faders, so test the observable contract instead of private implementation details.

## When To Use

- Adding a new test or changing an existing test.
- Fixing a bug or changing behavior that needs coverage.
- Choosing between Rust unit, actor, smoke, UI unit, Storybook, or visual coverage.
- Reviewing tests for brittle coupling or missing safety behavior.

## Core Rules

- Start from the behavior that must remain true.
- Keep Rust tests in one of the approved styles: pure unit tests, actor tests through mailboxes/events/tracing, or smoke tests through the debug module/app.
- Do not test side-effecting actors by mutating internals or inspecting private state.
- Do not write source-string tests with `include_str!` or similar implementation-text assertions.
- Use frontend tests for UI behavior and Storybook stories/tests for relevant UI states.
- Prefer targeted tests over broad tests when the behavior is local.

## Rust Test Selection

| Behavior | Test style |
|---|---|
| Pure formatting, parsing, matching, or small policy function | Pure unit test |
| Actor command, event, projected fact, or tracing behavior | Actor test through mailbox, `AppEventBus`, and tracing listener when needed |
| End-to-end debug app behavior with LV1-compatible target | Smoke test |

## Frontend Test Selection

| Behavior | Test style |
|---|---|
| Component rendering, interaction, disabled states, formatting | Vitest/UI unit test |
| Important visual or stateful component variants | Storybook story plus Storybook test when interactive |
| Intentional visual design changes | Visual regression update when appropriate |

## Common Mistakes

- Testing actor internals instead of sending commands through the actor handle.
- Asserting on Rust source text instead of behavior.
- Adding UI logic tests for behavior that belongs in the backend.
- Covering only the happy path for safety-sensitive behavior.
- Using `cargo test` by habit when `cargo nextest run ...` is the project default.

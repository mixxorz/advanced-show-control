---
name: make-sure-it-works
description: Use when finishing SDD, implementation-plan work, feature work, bug fixes, or user-reported workflow fixes in this LV1 fade utility, before claiming completion or notifying the user.
---

# Make Sure It Works

## Overview

Completion means the user's workflow works through the real boundary that could fail. Green checks are not enough when they do not exercise the production path.

**Core principle:** prove the behavior at the boundary where the bug or feature lives, then state exactly what was and was not verified.

## When To Use

- Before ending any SDD task or implementation-plan task.
- Before saying a feature, bug fix, user workflow, or review item is complete.
- Before sending a completion notification.
- When a test uses mocks but the real workflow crosses Tauri commands, actors, event bus, projector, filesystem, LV1 state, or debug smoke app boundaries.
- When the user asks for a smoke test, end-to-end test, integration test, or says the feature should actually work.

## Completion Gate

Do not claim completion until you can fill this matrix from fresh evidence:

| Requirement | Proof command/test | Boundary proved | Boundary not proved | Output inspected |
|---|---|---|---|---|
| Primary user workflow | command/test name | real path crossed | remaining gap or none | yes/no |

Rules:

- The primary workflow row must assert the final observed state, not just that a handler or mock was called.
- If the workflow crosses backend state or projection, one proof must observe the projected `AppViewState` or rendered UI after the real command path.
- If the user says smoke test, this repo's default meaning is `make smoke`; inspect `logs/debug-smoke-report.txt` before calling it smoke-verified.
- Do not call Vitest, Storybook, Playwright, mocked AppRuntime tests, or `make check` a smoke test.
- If `make smoke` is not feasible, say `not smoke-verified` and explain the exact substitute boundary you did verify.
- A code review is not proof that the workflow works. Ask reviewers whether the tests would fail if the reported workflow still failed.

## Boundary Checklist

For the main workflow, identify every layer it depends on and mark each as proved or unproved:

| Layer | Example proof |
|---|---|
| UI interaction | user event changes rendered UI |
| Frontend command adapter | command wrapper called with production shape |
| Tauri command registration | command invoked through Tauri/debug app path |
| Actor/domain mutation | actor state changes through mailbox/handle |
| Show/file persistence | document or dirty state reflects mutation |
| Event bus/projector | `app-status-changed` or `AppViewState` contains update |
| Rendered projection | user-visible UI shows final state |
| LV1/hardware behavior | `make smoke` report or dedicated hardware smoke output |

You do not always need every layer. You must prove the layer where the feature could realistically fail.

## Smoke Means Smoke

In this repository, `smoke` is a protected word.

Use these labels accurately:

| Test type | Allowed label |
|---|---|
| `make smoke` debug Tauri app plus `logs/debug-smoke-report.txt` inspected | smoke-verified |
| Debug app path without LV1-dependent assertions | debug-app verified |
| Rust actor mailbox/event bus test | actor verified |
| Tauri command adapter test | command-boundary verified |
| React test with mocked services | frontend wiring verified |
| Storybook or Playwright screenshot | visual verified |

## Red Flags

Stop before completion if you are about to say:

- "The smoke test passes" but you did not run `make smoke`.
- "End-to-end" but a mock stands at the command/backend boundary.
- "It works" but the test only asserts a callback was called.
- "Review found no issues" but no test would fail if the user's workflow still failed.
- "make check passed" as proof of a specific user workflow.
- "Should be fixed" without observing the final projected/rendered state.

## Common Mistakes

| Mistake | Correction |
|---|---|
| Mocked service call treated as workflow proof | Add proof across the real command/projection boundary. |
| Broad verification treated as feature verification | Add a requirement-specific proof row. |
| Smoke requested but only frontend tests run | Run `make smoke` or report `not smoke-verified`. |
| Projection bug tested only at command call site | Wait for and assert projected state/UI after mutation. |
| Completion notification sent after partial proof | State gaps first; notify only after requested verification is complete. |

## Final Response Contract

When using this skill, final status must include:

- The primary workflow proof and the final state observed.
- The broad checks run, if any.
- Whether the work is smoke-verified, debug-app verified, command-boundary verified, actor verified, frontend wiring verified, or visual verified.
- Any unverified boundary in plain language.

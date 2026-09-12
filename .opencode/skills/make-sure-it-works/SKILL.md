---
name: make-sure-it-works
description: Use before claiming implementation, bug-fix, native UI, or user-workflow work is complete in this LV1 fade utility.
---

# Make Sure It Works

## Overview

Completion requires fresh evidence at the boundary where the behavior could fail. Broad green checks do not by themselves prove a specific workflow.

## When To Use

- Before saying a feature, bug fix, user workflow, or review item is complete.
- When a workflow crosses GPUI, native command dispatch, actors, `AppEventBus`, the projector, persistence, LV1 state, or hardware-smoke boundaries.
- When the user asks for a smoke, end-to-end, integration, native visual, or component test.

## Completion Gate

Fill this matrix from fresh evidence before claiming completion:

| Requirement | Proof command/test | Boundary proved | Boundary not proved | Output inspected |
|---|---|---|---|---|
| Primary user workflow | command/test name | real path crossed | remaining gap or none | yes/no |

Rules:

- Assert the final observed state, not only that a handler, sender, or mock was called.
- If behavior crosses domain state or projection, observe the actor result, projected `AppViewState`, or rendered GPUI state after the real command path.
- Use only the test styles allowed by `docs/coding-conventions.md`: pure unit tests, actor tests through mailboxes/`AppEventBus` (plus tracing when relevant), and smoke tests through `dev-tools/`. Native UI tests use `#[gpui_kit::test]`.
- Before changing or verifying production code, discover applicable `@cc` comments and ancestor `CONTRACTS` files with `cc-check list path/to/file.rs[:line]`. Check semantic compliance; `cc-check format` validates syntax and duplicate IDs only.
- A code review or `make check` is not requirement-specific workflow proof.

## Choose the Smallest Meaningful Proof

| Boundary | Preferred proof |
|---|---|
| Pure domain calculation | Targeted pure unit test with `cargo nextest run ...` |
| Actor mutation or safety behavior | Actor test through mailbox and `AppEventBus` |
| Logging behavior | Actor test with a tracing listener |
| GPUI behavior | `#[gpui_kit::test]` component test |
| Native appearance on macOS | `make visual-test` and inspected perceptual snapshot result |
| Production and development crates | `make check` |
| LV1/hardware workflow | `make smoke`, then inspect `logs/debug-smoke-report.txt` |

Use `make visual-update` only after inspecting intentional UI changes. Windows appearance requires manual acceptance where equivalent image rendering is unavailable.

## Smoke Means Hardware Smoke

In this repository, `make smoke` runs the non-GUI `advanced-show-control-smoke` CLI from `dev-tools/` against an LV1-compatible environment. Its terminal output is not authoritative. Always inspect `logs/debug-smoke-report.txt` before reporting the suite result.

Use these labels accurately:

| Evidence | Allowed label |
|---|---|
| `make smoke` plus authoritative report inspected | smoke-verified |
| Actor mailbox/event-bus test | actor verified |
| GPUI Kit component test | native component verified |
| `make visual-test` with output inspected | native visual verified on macOS |
| Broad format, lint, test, and build checks | `make check` passed |

If hardware smoke is not feasible, say `not smoke-verified` and name the substitute boundary that was tested.

## Common Commands

```bash
cargo nextest run -p advanced-show-control <test-filter>
cargo nextest run --manifest-path dev-tools/Cargo.toml
make visual-test
make check
make smoke
```

## Red Flags

Stop before completion if you are about to say:

- “Smoke passed” without running `make smoke` and reading `logs/debug-smoke-report.txt`.
- “End-to-end” when a mock replaces a boundary relevant to the failure.
- “It works” when the test only records command dispatch.
- “Visual verified” without running and inspecting the native visual test.
- “`make check` passed” as the only proof of a specific workflow.
- “Should be fixed” without observing the final domain, projected, rendered, or hardware state.

## Final Response Contract

Report:

- The primary workflow proof and final state observed.
- Every command run and whether it passed.
- The accurate verification label, if applicable.
- Any unverified boundary or platform-specific risk in plain language.

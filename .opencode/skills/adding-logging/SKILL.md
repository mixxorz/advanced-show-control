---
name: adding-logging
description: Use when adding, changing, reviewing, or relying on application logs, tracing events, frontend log UI messages, diagnostics, or user-facing operational messages.
---

# Adding Logging

## Overview

Logs are both diagnostics and user-visible operational facts. Every application log needs a stable event name and a complete human-readable message.

## When To Use

- Adding or changing `tracing` calls.
- Making safety blocks, command failures, file operations, connection progress, or state changes visible.
- Deciding log level or log delivery path.
- Reviewing duplicated, noisy, or sensitive log output.

## Core Rules

- Use `tracing` as the application logging API.
- Include a stable `event` field on every application log.
- Write user-facing messages that are understandable without structured fields.
- Use structured fields for diagnostics, filtering, and support logs, not as the only explanation.
- Do not dump full settings files, show files, OSC payloads, or other noisy/sensitive state into user-facing logs.
- Do not duplicate the same fact at multiple layers.

## Level Selection

| Level | Use for |
|---|---|
| `DEBUG` | Protocol details, internal decisions, noisy diagnostics, subscriber lag, state counts, low-level writes/drops, no-ops, shutdown details |
| `INFO` | User-relevant successful operations and state changes |
| `WARN` | Visible safety blocks, skipped or blocked operations, recoverable failures, invalid user-owned files with safe fallback |
| `ERROR` | Command failures, unrecoverable runtime setup failures, failed writes that prevent diagnostics or requested persistence |

## Delivery Rules

- Runtime modules emit tracing events only.
- Do not publish `AppEventBus` events solely to create logs.
- `DEBUG` and above go to diagnostic logs.
- `INFO`, `WARN`, and `ERROR` project into frontend log state through the tracing UI sink.
- The frontend receives log state only through `app-status-changed` snapshots.

## Common Mistakes

- Logging a user-visible fact at `DEBUG` so it never reaches the UI.
- Repeating the same outcome in adapter, actor, and projector logs.
- Writing terse messages that require reading structured fields to understand.
- Logging sensitive or noisy payloads for convenience.
- Creating event-bus facts just to make logs appear.

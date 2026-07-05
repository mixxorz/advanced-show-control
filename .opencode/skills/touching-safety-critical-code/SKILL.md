---
name: touching-safety-critical-code
description: Use when touching LV1 state, scene recall, fade execution, fader writes, lockout, disconnect or reconnect handling, generation guards, stale state, or manual override behavior.
---

# Touching Safety Critical Code

## Overview

Safety checks are part of the mixer-control contract. Do not send fader commands unless the current LV1 state and recall/fade policy make that action safe.

## When To Use

- Changing LV1 connection or mirrored state behavior.
- Changing scene recall automation or exact scene identity matching.
- Changing fade execution, fader write timing, manual override, abort, overlap, or same-scene behavior.
- Changing disconnect, reconnect, generation guard, stale task, or stale state handling.
- Changing lockout or safety-block behavior.

## Non-Negotiable Invariants

- Do not bypass lockout checks.
- Do not bypass exact scene identity validation unless the task explicitly changes the matching model.
- Do not bypass generation guards.
- Stale tasks must not send fader commands or write misleading UI logs after disconnect or reconnect.
- Do not send fader commands when LV1 state is unavailable, disconnected, stale, or unsafe.
- Scene recall automation must validate before aborting an existing fade.
- Blocked, skipped, or disabled recalls must not abort an existing fade.
- Use fresh LV1 state for recall automation where event subscriber ordering could otherwise create stale decisions.
- Make safety blocks visible through logs, facts, or projected UI state.
- Preserve manual override, abort, overlap/same-scene, and disconnect safety behavior.

## Safety Review Questions

- What fader command could this path send, and what proves LV1 state is safe at that moment?
- Could a stale async task still act after disconnect or reconnect?
- Could a blocked recall accidentally abort an existing fade?
- Is the safety block visible to the engineer?
- Does the change preserve exact scene identity unless explicitly changing the model?

## Common Mistakes

- Validating once and assuming the result remains fresh across async boundaries.
- Aborting an existing fade before proving the new recall is valid.
- Treating skipped or disabled recall as harmless while still changing fade state.
- Letting generation mismatches suppress commands but still emit misleading success logs.
- Moving safety checks to the UI instead of enforcing them in backend-owned policy.

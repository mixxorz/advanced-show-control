# Code Review Remediation Design

## Scope

This pass finishes the remaining review gaps after `main` was updated with the earlier remediation commits. The ordering follows the pasted remaining-items list, with `CODE_REVIEW.md` as the source for the underlying findings and current `main` as the source for what still needs work.

The pass is limited to review remediation. It does not start Phase 8/9 external control work or redesign the product workflow.

## 1. Blocking mDNS On Connect

`connect_lv1` still calls `resolve_target` directly on the async command path when no host is provided. Move that work into `spawn_blocking`, matching the discovery refresh path, and keep the existing error strings and command response shape.

Success means a manual connect with discovery timeout no longer blocks a Tauri async runtime worker.

## 2. Generation-Guard Hardening

The current bus has `start_fade_if_generation`, and `clear_targets` bumps generation, but `get_generation`, `set_generation`, and unchecked `start_fade` still make check-then-act easy to reintroduce. `FadeConfig` and `FadeCommand::RecallSceneFade` also carry no generation, so a fade command obtained outside the bus cannot be re-verified downstream.

Harden the invariant by making generation part of recall fade dispatch. Prefer the smallest API change that keeps CLI/test callers usable: preserve a direct manual/test start path only where it is clearly not a stale recall path, and route scene recall through generation-bearing or generation-checked APIs. Add tests for stale generation before dispatch and generation flip between policy decision and fade start.

## 3. Misleading And Missing Tests

Replace tests that assert implementation trivia with behavioral tests:

- Replace the tautological pan-address test with assertions that actual `Lv1ParameterWrite` commands encode to the expected OSC paths.
- Remove or differentiate the byte-identical duplicate flush test so each flush test protects a distinct behavior.
- Replace the unconditional `Notify` in `tests/runtime_bus.rs` with a real readiness condition from LV1 state/events.
- Add missing tests for fader override preserving pan-family targets and per-parameter send deltas.

Success means tests fail for the bugs described in `CODE_REVIEW.md`, not only for string-literal drift.

## 4. Show-File Mapping And Backups

Add a structural export-to-load mapping round-trip that exercises the hand-written `export_show_file` and `load_show_file_from_dto` paths, not only serde. Rename or wrap `validate_show_file` so pruning behavior is explicit at call sites.

Add bounded backup retention for the app backup directory. Keep the existing backup-before-overwrite behavior, but prune older backups for the same show file after a successful new backup. The bound should be small and documented in code; tests should prove old backups are removed and unrelated backups are retained.

## 5. Scene ID Residue

Use the centralized scene-id helpers instead of allowing silent parse fallback to `0::""`. Production storage should reject invalid scene IDs with a clear error instead of creating junk configs. Test helpers should construct valid IDs or fail loudly.

## 6. Small Cleanups

Centralize duplicate timestamp helpers where doing so does not create awkward crate coupling. For core actor bus lag currently going to stderr only, route it to an existing visible diagnostic/log surface if one already exists; otherwise document the remaining gap rather than inventing a broad logging subsystem.

## 7. Documentation

Update docs to match the actual runtime contracts:

- `docs/architecture.md`: document `write_batch` fire-and-forget behavior while disconnected if that remains the chosen trade-off, and document generation-checked recall dispatch.
- `docs/scene-tracking.md`: include recall timing-window rationale and the 2 s fresh-snapshot wait/gating behavior.

## 8. UI Quality Basket

Keep UI quality work deliberately small:

- Document the `ui/src/types.ts` and Rust `AppViewState` sync contract, or add a lightweight generated/checked artifact if the existing toolchain supports it without introducing a new build system.
- Normalize Header and Scene tab scene index display semantics, after confirming whether hardware index values are zero-based internally.
- Add a minimal frontend quality foothold that fits the current package: at minimum keep `npm run typecheck` and `npm run build` green. Add ESLint or UI tests only if it is a small, well-contained package change; otherwise record it as follow-up instead of expanding this remediation pass.

## Verification

Use targeted tests while editing, then run the smallest broad verification that proves the remediation:

- Rust formatting and targeted nextest packages for changed runtime areas.
- `cargo nextest run --workspace` before claiming completion.
- `npm run typecheck` and `npm run build` for UI changes.

## Non-Goals

- No Phase 8 HTTP/WebSocket API work.
- No Companion integration.
- No broad UI redesign.
- No logging-system rewrite unless an existing visible route is already available.

# Task 1 report: shell status and layout refinement

Status: DONE

Commit: this report is included in the task commit; the commit hash is reported with the final task result.

## Scope completed

- Removed `MainTab::Events`, the Events tab, and its placeholder only.
- Added projected Connected/Connecting/Disconnected presentation mapping with green `STATUS_CUED`, amber `STATUS_WARNING`, and red `STATUS_DANGER` dots while retaining neutral-primary `CONNECTED`, `CONNECTING`, and `OFFLINE` labels.
- Converted the console chooser to one GPUI Kit component button with its built-in trailing dropdown caret and the existing connection-dialog callback.
- Allocated approximately 14% of the footer to GO, made the button fill most of that section, and gave CUED/CURRENT/MODE/TIME equal zero-basis growth.
- Preserved the existing GO callback, single-flight guard, disabled-state guards, session-menu behavior, modals, projector state, actors, event infrastructure, generation handling, recall, and safety behavior.
- Updated GPUI behavior/geometry coverage, all nine shared-shell visual signatures, the application-shell documentation, and its synchronized screenshot.

## Contracts

`cc-check` was unavailable. I manually reviewed `app/src/CONTRACTS` and the local `go-single-flight` and `cued-scene-resolution` contracts before editing. The change remains presentation-only, reads connection state from the projected snapshot, leaves GO dispatch and scene resolution unchanged, and does not weaken or remove any contract.

## RED-GREEN-REFACTOR evidence

### RED

- `cargo nextest run -p advanced-show-control native_ui::shell::tests` failed with unresolved import `connection_presentation`, proving the new state-to-label/color behavior did not exist.
- `make visual-test` failed at `window.try_find("tab-Events").is_none()`, proving the Events tab remained. The added harness assertions also require the new connection IDs, footer IDs, 8 px dot, dialog interaction, 13–15% GO section, 80%/75% GO fill, and equal status-cell widths.

### GREEN

- After the minimal shell implementation, the focused shell suite passed 3/3 tests.
- The native visual harness passed its navigation, status-dot, console-dialog, footer geometry, session-menu, modal, GO/Cue guard, focus, and snapshot checks.

### REFACTOR

- Consolidated connection label/color selection in `connection_presentation` and reused `status_cell` for stable IDs and equal flex behavior.
- Reviewed the passing implementation for unnecessary abstractions or duplicated policy. No broader refactor was needed.

## Visual review and reference comparison

Inspected every affected generated PNG:

- `native-connection.png`
- `native-shell-ready.png`
- `native-session-menu.png`
- `native-cue-lists.png`
- `native-cue-manager.png`
- `native-settings.png`
- `native-logs.png`
- `native-shell-safe.png`
- `native-scene-overwrite.png`

Direct normalized comparison of `dist/visual/native-shell-ready.png` with `/Users/mixxorz/Downloads/Codex Image Sep 14, 2026, 01_18_47 AM.png`:

- Navigation order: aligned, except Events is intentionally absent per the approved design.
- Connection status: aligned; an inline circular green dot precedes a neutral-primary `CONNECTED` label. The offline capture shows the corresponding red dot with neutral-primary `OFFLINE`, and the pure mapping test covers the amber dot and neutral-primary `CONNECTING` label.
- Console chooser: aligned; the console name and trailing down-caret are one button.
- GO prominence: aligned; GO is substantially larger and fills its dedicated footer section.
- Footer distribution: aligned; GO occupies about 14%, and the four status cells divide the remainder evenly.
- Menu and overlays: session-menu appearance, connection dialog, cue manager, overwrite confirmation, and dimmed controls remain coherent.
- SAFE state: retained and visually coherent.

Intentional differences from the reference:

- Events is removed by design even though it appears in the reference.
- The current app uses its existing scene editor and projected reference data rather than the reference image's content.
- The headless visual harness includes a command-failure notification in connected captures.
- Native/headless window chrome and exact typography differ from the external reference.

The site screenshot was copied from the final follow-up ready capture; both files had SHA-256 `de31d7e8ce6ecd9866843cf9a630bf06b6d89a03fcd072360e2500752905f2a5` at synchronization.

## Final verification

Executed successfully as one chain:

```text
cargo fmt --all -- --check
cargo nextest run -p advanced-show-control native_ui::shell::tests
make check
make visual-test
```

Evidence:

- Focused shell tests: 3 passed, 0 failed.
- Production workspace tests in `make check`: 554 passed, 0 failed.
- Development-tool tests: 28 passed, 0 failed.
- Production and development-tool formatting, Clippy with `-D warnings`, and builds passed.
- Final native visual test passed with reviewed signatures.

The only notice was the existing Rust future-incompatibility warning for dependency `block v0.1.6`.

## Changed files

- `app/src/native_ui/state.rs`
- `app/src/native_ui/shell.rs`
- `app/src/native_ui/visual.rs`
- `app/src/native_ui/visual_snapshots/native-connection.rgb`
- `app/src/native_ui/visual_snapshots/native-cue-lists.rgb`
- `app/src/native_ui/visual_snapshots/native-cue-manager.rgb`
- `app/src/native_ui/visual_snapshots/native-logs.rgb`
- `app/src/native_ui/visual_snapshots/native-scene-overwrite.rgb`
- `app/src/native_ui/visual_snapshots/native-session-menu.rgb`
- `app/src/native_ui/visual_snapshots/native-settings.rgb`
- `app/src/native_ui/visual_snapshots/native-shell-ready.rgb`
- `app/src/native_ui/visual_snapshots/native-shell-safe.rgb`
- `site/docs/application-shell.md`
- `site/docs/assets/screenshots/application-shell.png`
- `.superpowers/sdd/shell-refinement/task-1-report.md`

## Whole-branch review follow-up

The final whole-branch review requested two fixes, completed in a separate follow-up commit:

- The visual harness now sends `MENU_NEW_SHORTCUT` and checks the dispatcher increment immediately after closing the startup dialog. It checks `NewShow` and `Quit` action availability at that same point, before any synthetic pointer click. Only after that regression is proven does the harness click the console chooser, assert that the existing dialog opens, and dismiss it.
- Connection presentation now maps a separate neutral `CONSOLE_PRIMARY` label color for all three states. Green, amber, and red remain exclusive to the 8 px status dot.

Follow-up RED evidence:

- `cargo nextest run -p advanced-show-control native_ui::shell::tests` failed with three tuple-size mismatches because the test required label, dot color, and neutral label color while `connection_presentation` still returned only label and status color.
- The reordered `make visual-test` passed before the rendering change, confirming that the immediate-shortcut and chooser checks remained behaviorally valid while establishing the required interaction order.

Follow-up GREEN evidence:

- `cargo nextest run -p advanced-show-control native_ui::shell::tests` passed 3/3 after adding neutral label color to the mapping and rendering.
- `make visual-test` passed with the shortcut assertion before any chooser click and with the chooser interaction still covered separately.
- All nine affected captures were regenerated and inspected. Connected and offline labels are neutral, status colors remain on dots only, and navigation, GO geometry, menu, modal, SAFE, and footer behavior remain unchanged.
- The final ready capture was compared directly with the supplied reference. The connection treatment now matches the reference more closely; previously documented intentional differences remain Events removal, projected fixture content, the harness notification, and native/headless chrome and typography.

Follow-up final verification passed as one fresh chain:

```text
cargo nextest run -p advanced-show-control native_ui::shell::tests
cargo fmt --all -- --check
make check
make visual-test
```

The focused suite passed 3/3, the production suite passed 554/554, the development-tool suite passed 28/28, formatting/Clippy/builds passed, and the reviewed visual signatures passed.

## Concerns

No implementation concerns. `cc-check` remained unavailable, so contract verification was manual as required. The dependency future-incompatibility warning is pre-existing and outside this task's scope.

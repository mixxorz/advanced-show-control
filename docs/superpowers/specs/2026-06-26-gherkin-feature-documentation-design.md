# Gherkin Feature Documentation Design

## Purpose

Document the app's implemented product behavior as organized Gherkin feature files under `features/`.

The feature files are product documentation, not an executable test suite in this slice. They should describe behavior from the live engineer's perspective and preserve the app's safety-critical intent.

## Scope

Include implemented behavior only.

Do not add scenarios for planned roadmap features that are not implemented. Cue Lists and Events are currently placeholder tabs, so they should not receive feature files in this pass.

## Organization

Use one Gherkin feature file per distinct implemented product capability.

Use subdirectories only to keep related capabilities easy to browse. The subdirectory is not the feature boundary; the `Feature:` declaration inside each file is the boundary.

Create feature files under `features/` using stable, lower-case file names:

- `features/connection/lv1-discovery.feature`
- `features/connection/manual-lv1-connection.feature`
- `features/connection/lv1-disconnection.feature`
- `features/connection/lv1-reconnect.feature`
- `features/connection/startup-auto-connect.feature`
- `features/sessions/new-session.feature`
- `features/sessions/open-session.feature`
- `features/sessions/save-session.feature`
- `features/sessions/session-title.feature`
- `features/sessions/scene-alignment.feature`
- `features/scenes/scene-list.feature`
- `features/scenes/scene-selection.feature`
- `features/scenes/scene-cueing.feature`
- `features/scenes/scene-recall.feature`
- `features/scenes/store-scene-config.feature`
- `features/scenes/link-scene-config.feature`
- `features/scenes/delete-scene-config.feature`
- `features/scenes/scene-duration.feature`
- `features/scopes/fader-scope.feature`
- `features/scopes/pan-scope.feature`
- `features/scopes/channel-scope.feature`
- `features/fades/timed-fade.feature`
- `features/fades/zero-duration-fade.feature`
- `features/fades/scoped-parameter-fade.feature`
- `features/fades/fade-overlap.feature`
- `features/fades/same-scene-repeat.feature`
- `features/fades/manual-override.feature`
- `features/safety/lockout.feature`
- `features/safety/abort-all.feature`
- `features/safety/recall-safety.feature`
- `features/safety/connection-loss-safety.feature`
- `features/safety/scene-mismatch-safety.feature`
- `features/settings/app-settings.feature`
- `features/settings/keyboard-shortcuts.feature`
- `features/settings/settings-persistence.feature`
- `features/logs/operational-logs.feature`

This structure keeps each Gherkin file focused on a single product capability while avoiding tight coupling to React components or Rust actor internals.

## Feature Coverage

The connection files should cover LV1 discovery, startup connection modal behavior, selecting an available console, connected and unavailable discovery rows, disconnect, reconnect overlay behavior, and startup auto-connect where currently implemented.

The session files should cover creating a new `.ascs` session from current LV1 state, opening a session, saving, Save As, untitled defaults, dirty window title state, native file menu behavior where visible through the app, and scene alignment behavior when loaded app-managed scene configs do not line up with the current LV1 scene list.

The scene files should cover scene list display, current/cued/selected scene state, selecting and cueing scenes, duplicate-name warnings, recalling scenes through the app, storing app-managed scene configs from LV1, linking unlinked scene configs, overwrite confirmation, deleting scene configs, and duration editing.

The scope files should cover fader and pan scope toggles, individual channel scope, and all-channel scope changes.

The fade files should cover recalling app-managed scenes, starting fades from current live values, duration-based movement, immediate movement for zero-duration scenes, scoped-only movement, final target behavior, overlap behavior, same-scene repeat behavior, and manual override cancellation.

The safety files should cover global lockout, Abort All, blocked recall visibility, disconnected or unavailable LV1 state, scene mismatch behavior, connection-loss and reconnect behavior, and the rule that blocked, skipped, or disabled recalls do not abort an active fade.

The settings files should cover implemented settings controls: auto-load last show file, auto-save sessions, auto-cue next scene on GO as a stored setting, time display preference, fader override sensitivity, GO shortcut capture, CUE shortcut capture, immediate settings persistence, optimistic UI behavior, and settings command failure display.

The logs file should cover frontend-facing operational logs, visible safety warnings, bounded log projection through app state, and separation from diagnostic-only debug logs.

## Gherkin Style

Each file should use standard Gherkin syntax:

```gherkin
Feature: Short product capability
  As a live engineer
  I want ...
  So that ...

  Scenario: Observable behavior
    Given ...
    When ...
    Then ...
```

Keep scenarios concise and behavior-focused. Avoid implementation names such as actor type names, command enum names, or React component names unless they are visible product language.

Use consistent vocabulary:

- `LV1` for Waves eMotion LV1 or LV1 Classic.
- `session` for `.ascs` app show files.
- `app-managed scene` for an LV1 scene with stored app fade metadata.
- `scoped channel` or `scoped parameter` for values the app is allowed to move.
- `lockout` and `Abort All` for safety controls.

Prefer `Rule:` blocks where a feature has safety invariants that apply to several scenarios.

## Non-Goals

Do not add a Cucumber runner, step definitions, CI integration, or executable acceptance-test plumbing.

Do not document future Cue List, Event Automation, external API, or Stream Deck behavior.

Do not add broad architectural commentary to the feature files. Architecture remains in `docs/architecture.md`.

## Verification

Because this is documentation-only, verification should confirm that the feature files exist, use `.feature` syntax, and contain only implemented behavior. A full build is not required for this slice unless code changes are introduced.

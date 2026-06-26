# Gherkin Feature Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create documentation-grade Gherkin feature files for every implemented app capability under `features/`.

**Architecture:** This is a documentation-only change. Each `.feature` file documents one implemented product capability from the live engineer's perspective. Safety scenarios are documented inside the feature files whose behavior they constrain, not in a separate safety folder.

**Tech Stack:** Gherkin `.feature` files, Markdown planning docs, existing repository documentation in `docs/`.

## Global Constraints

- Include implemented behavior only.
- Do not add scenarios for planned roadmap features that are not implemented.
- Do not create feature files for Cue Lists or Events in this pass.
- Use one Gherkin feature file per distinct implemented product capability.
- Use subdirectories only to keep related capabilities easy to browse.
- The `Feature:` declaration inside each file is the feature boundary.
- Keep scenarios concise and behavior-focused.
- Avoid implementation names such as actor type names, command enum names, or React component names unless they are visible product language.
- Use `LV1` for Waves eMotion LV1 or LV1 Classic.
- Use `session` for `.ascs` app show files.
- Use `app-managed scene` for an LV1 scene with stored app fade metadata.
- Use `scoped channel` or `scoped parameter` for values the app is allowed to move.
- Use `lockout` and `Abort All` for safety controls.
- Prefer `Rule:` blocks where a feature has safety invariants that apply to several scenarios.
- Do not add a Cucumber runner, step definitions, CI integration, or executable acceptance-test plumbing.
- Do not add broad architectural commentary to the feature files.

---

## File Structure

Create these files:

- `features/connection/lv1-discovery.feature`: LV1 discovery list, available/unavailable/connected rows, latency display.
- `features/connection/manual-lv1-connection.feature`: selecting an available LV1 system through the connection modal.
- `features/connection/lv1-disconnection.feature`: disconnecting from LV1 and preventing further live operations until reconnected.
- `features/connection/lv1-reconnect.feature`: reconnect overlay and connection-loss safety.
- `features/connection/startup-auto-connect.feature`: startup connection workflow and auto-connect behavior.
- `features/sessions/new-session.feature`: creating a new `.ascs` session from current LV1 state.
- `features/sessions/open-session.feature`: opening a saved `.ascs` session.
- `features/sessions/save-session.feature`: save, Save As, untitled default naming, and dirty state clearing.
- `features/sessions/session-title.feature`: window title session name and dirty marker behavior.
- `features/sessions/scene-alignment.feature`: scene alignment when loaded app-managed scene configs do not line up with the current LV1 scene list.
- `features/scenes/scene-list.feature`: scene list display and duplicate-name warnings.
- `features/scenes/scene-selection.feature`: selecting app-managed scenes for editing.
- `features/scenes/scene-cueing.feature`: cueing a scene without recalling it.
- `features/scenes/scene-recall.feature`: recalling scenes through the app, including recall safety rules.
- `features/scenes/store-scene-config.feature`: storing app-managed scene configs from live LV1 state.
- `features/scenes/link-scene-config.feature`: linking unlinked configs and overwrite confirmation.
- `features/scenes/delete-scene-config.feature`: deleting app-managed scene configs.
- `features/scenes/scene-duration.feature`: editing scene fade durations.
- `features/scopes/fader-scope.feature`: enabling/disabling fader scope for a scene.
- `features/scopes/pan-scope.feature`: enabling/disabling pan-family scope for a scene.
- `features/scopes/channel-scope.feature`: scoped channel selection and all-channel changes.
- `features/fades/timed-fade.feature`: timed fades from current live values to stored targets.
- `features/fades/zero-duration-fade.feature`: immediate movement for zero-duration scenes.
- `features/fades/scoped-parameter-fade.feature`: moving only scoped parameters.
- `features/fades/fade-overlap.feature`: overlap behavior and blocked recalls not aborting active fades.
- `features/fades/same-scene-repeat.feature`: same-scene repeat behavior.
- `features/fades/manual-override.feature`: manual override cancellation.
- `features/controls/lockout.feature`: global lockout behavior.
- `features/controls/abort-all.feature`: Abort All behavior.
- `features/settings/app-settings.feature`: app settings controls.
- `features/settings/keyboard-shortcuts.feature`: GO and CUE shortcut capture.
- `features/settings/settings-persistence.feature`: immediate settings persistence and failure display.
- `features/logs/operational-logs.feature`: frontend-facing operational logs and diagnostic separation.

No existing code files should be modified.

---

### Task 1: Connection And Session Feature Files

**Files:**
- Create: `features/connection/lv1-discovery.feature`
- Create: `features/connection/manual-lv1-connection.feature`
- Create: `features/connection/lv1-disconnection.feature`
- Create: `features/connection/lv1-reconnect.feature`
- Create: `features/connection/startup-auto-connect.feature`
- Create: `features/sessions/new-session.feature`
- Create: `features/sessions/open-session.feature`
- Create: `features/sessions/save-session.feature`
- Create: `features/sessions/session-title.feature`
- Create: `features/sessions/scene-alignment.feature`

**Interfaces:**
- Consumes: approved spec `docs/superpowers/specs/2026-06-26-gherkin-feature-documentation-design.md`.
- Produces: connection and session `.feature` files with valid Gherkin `Feature:`, optional `Rule:`, and `Scenario:` blocks.

- [ ] **Step 1: Create connection feature files**

Use `apply_patch` to add the five connection files. Include these exact feature titles:

```gherkin
Feature: LV1 discovery
Feature: Manual LV1 connection
Feature: LV1 disconnection
Feature: LV1 reconnect
Feature: Startup auto-connect
```

Each file must include at least two scenarios. `lv1-reconnect.feature` must include a `Rule:` stating that stale connection work must not affect the active connection.

- [ ] **Step 2: Create session feature files**

Use `apply_patch` to add the five session files. Include these exact feature titles:

```gherkin
Feature: New session
Feature: Open session
Feature: Save session
Feature: Session title
Feature: Scene alignment
```

`scene-alignment.feature` must describe loaded app-managed scene configs that do not line up with the current LV1 scene list and the visible alignment/skipped-config outcome. Do not describe future Cue List, Event Automation, external API, or Stream Deck behavior.

- [ ] **Step 3: Verify Task 1 files exist**

Run: `test -f features/connection/lv1-discovery.feature && test -f features/connection/manual-lv1-connection.feature && test -f features/connection/lv1-disconnection.feature && test -f features/connection/lv1-reconnect.feature && test -f features/connection/startup-auto-connect.feature && test -f features/sessions/new-session.feature && test -f features/sessions/open-session.feature && test -f features/sessions/save-session.feature && test -f features/sessions/session-title.feature && test -f features/sessions/scene-alignment.feature`

Expected: command exits `0` with no output.

- [ ] **Step 4: Commit Task 1**

Run:

```bash
git add features/connection features/sessions
git commit -m "docs: add connection and session features"
```

Expected: commit succeeds and includes only Task 1 feature files.

---

### Task 2: Scene, Scope, Fade, And Control Feature Files

**Files:**
- Create: `features/scenes/scene-list.feature`
- Create: `features/scenes/scene-selection.feature`
- Create: `features/scenes/scene-cueing.feature`
- Create: `features/scenes/scene-recall.feature`
- Create: `features/scenes/store-scene-config.feature`
- Create: `features/scenes/link-scene-config.feature`
- Create: `features/scenes/delete-scene-config.feature`
- Create: `features/scenes/scene-duration.feature`
- Create: `features/scopes/fader-scope.feature`
- Create: `features/scopes/pan-scope.feature`
- Create: `features/scopes/channel-scope.feature`
- Create: `features/fades/timed-fade.feature`
- Create: `features/fades/zero-duration-fade.feature`
- Create: `features/fades/scoped-parameter-fade.feature`
- Create: `features/fades/fade-overlap.feature`
- Create: `features/fades/same-scene-repeat.feature`
- Create: `features/fades/manual-override.feature`
- Create: `features/controls/lockout.feature`
- Create: `features/controls/abort-all.feature`

**Interfaces:**
- Consumes: Task 1 vocabulary and directory structure.
- Produces: scene, scope, fade, and control `.feature` files with safety scenarios folded into related behavior.

- [ ] **Step 1: Create scene feature files**

Use `apply_patch` to add the eight scene files. Include these exact feature titles:

```gherkin
Feature: Scene list
Feature: Scene selection
Feature: Scene cueing
Feature: Scene recall
Feature: Store scene configuration
Feature: Link scene configuration
Feature: Delete scene configuration
Feature: Scene duration
```

`scene-recall.feature` must include blocked recall scenarios for lockout, disconnected LV1, scene mismatch, and unavailable scene state. It must state that a blocked recall does not abort an active fade.

- [ ] **Step 2: Create scope feature files**

Use `apply_patch` to add the three scope files. Include these exact feature titles:

```gherkin
Feature: Fader scope
Feature: Pan scope
Feature: Channel scope
```

Each file must describe how scoped behavior affects what the app is allowed to move.

- [ ] **Step 3: Create fade feature files**

Use `apply_patch` to add the six fade files. Include these exact feature titles:

```gherkin
Feature: Timed fade
Feature: Zero-duration fade
Feature: Scoped parameter fade
Feature: Fade overlap
Feature: Same-scene repeat
Feature: Manual override
```

`fade-overlap.feature` must include the safety invariant that blocked, skipped, or disabled recalls do not abort an active fade. `manual-override.feature` must describe cancellation when a live engineer moves a controlled parameter during a fade.

- [ ] **Step 4: Create control feature files**

Use `apply_patch` to add the two control files. Include these exact feature titles:

```gherkin
Feature: Lockout
Feature: Abort All
```

`lockout.feature` must describe user-visible blocking of recall or fade-starting operations. `abort-all.feature` must describe stopping active fades without changing unrelated LV1 state.

- [ ] **Step 5: Verify Task 2 files exist**

Run: `test -f features/scenes/scene-list.feature && test -f features/scenes/scene-selection.feature && test -f features/scenes/scene-cueing.feature && test -f features/scenes/scene-recall.feature && test -f features/scenes/store-scene-config.feature && test -f features/scenes/link-scene-config.feature && test -f features/scenes/delete-scene-config.feature && test -f features/scenes/scene-duration.feature && test -f features/scopes/fader-scope.feature && test -f features/scopes/pan-scope.feature && test -f features/scopes/channel-scope.feature && test -f features/fades/timed-fade.feature && test -f features/fades/zero-duration-fade.feature && test -f features/fades/scoped-parameter-fade.feature && test -f features/fades/fade-overlap.feature && test -f features/fades/same-scene-repeat.feature && test -f features/fades/manual-override.feature && test -f features/controls/lockout.feature && test -f features/controls/abort-all.feature`

Expected: command exits `0` with no output.

- [ ] **Step 6: Commit Task 2**

Run:

```bash
git add features/scenes features/scopes features/fades features/controls
git commit -m "docs: add scene fade and control features"
```

Expected: commit succeeds and includes only Task 2 feature files.

---

### Task 3: Settings, Logs, And Final Verification

**Files:**
- Create: `features/settings/app-settings.feature`
- Create: `features/settings/keyboard-shortcuts.feature`
- Create: `features/settings/settings-persistence.feature`
- Create: `features/logs/operational-logs.feature`

**Interfaces:**
- Consumes: Tasks 1 and 2 feature-file vocabulary.
- Produces: remaining implemented feature documentation and final verification evidence.

- [ ] **Step 1: Create settings feature files**

Use `apply_patch` to add the three settings files. Include these exact feature titles:

```gherkin
Feature: App settings
Feature: Keyboard shortcuts
Feature: Settings persistence
```

`app-settings.feature` must cover auto-load last show file, auto-save sessions, auto-cue next scene on GO as a stored setting, time display preference, and fader override sensitivity. `settings-persistence.feature` must cover immediate save, optimistic UI behavior, and command failure display.

- [ ] **Step 2: Create logs feature file**

Use `apply_patch` to add `features/logs/operational-logs.feature` with this exact title:

```gherkin
Feature: Operational logs
```

The file must describe frontend-facing `INFO`, `WARN`, and `ERROR` operational logs, visible safety warnings, bounded projection through app state, and separation from diagnostic-only debug logs.

- [ ] **Step 3: Verify all planned feature files exist**

Run: `git ls-files --others --cached -- 'features/**/*.feature' | sort`

Expected: output lists exactly 33 files:

```text
features/connection/lv1-disconnection.feature
features/connection/lv1-discovery.feature
features/connection/lv1-reconnect.feature
features/connection/manual-lv1-connection.feature
features/connection/startup-auto-connect.feature
features/controls/abort-all.feature
features/controls/lockout.feature
features/fades/fade-overlap.feature
features/fades/manual-override.feature
features/fades/same-scene-repeat.feature
features/fades/scoped-parameter-fade.feature
features/fades/timed-fade.feature
features/fades/zero-duration-fade.feature
features/logs/operational-logs.feature
features/scenes/delete-scene-config.feature
features/scenes/link-scene-config.feature
features/scenes/scene-cueing.feature
features/scenes/scene-duration.feature
features/scenes/scene-list.feature
features/scenes/scene-recall.feature
features/scenes/scene-selection.feature
features/scenes/store-scene-config.feature
features/scopes/channel-scope.feature
features/scopes/fader-scope.feature
features/scopes/pan-scope.feature
features/sessions/new-session.feature
features/sessions/open-session.feature
features/sessions/save-session.feature
features/sessions/scene-alignment.feature
features/sessions/session-title.feature
features/settings/app-settings.feature
features/settings/keyboard-shortcuts.feature
features/settings/settings-persistence.feature
```

- [ ] **Step 4: Verify no excluded roadmap features were documented**

Run: `rg -n "Cue Lists|Event Automation|Stream Deck|external API" features || true`

Expected: no output.

- [ ] **Step 5: Verify no standalone safety folder exists**

Run: `test ! -d features/safety`

Expected: command exits `0` with no output.

- [ ] **Step 6: Verify each feature file has a Feature declaration**

Run: `for file in features/**/*.feature; do rg -q '^Feature: ' "$file" || exit 1; done`

Expected: command exits `0` with no output.

- [ ] **Step 7: Commit Task 3**

Run:

```bash
git add features/settings features/logs
git commit -m "docs: add settings and logs features"
```

Expected: commit succeeds and includes only Task 3 feature files.

- [ ] **Step 8: Final status check**

Run: `git status --short`

Expected: no uncommitted changes from this feature-documentation work. Unrelated pre-existing files may remain and must not be modified or committed.

---

## Self-Review Notes

Spec coverage: Tasks cover all approved file paths and feature coverage categories from the design spec.

Placeholder scan: The plan contains no placeholder implementation instructions for the feature files; each task specifies exact files, exact titles, required scenario topics, commands, and expected results.

Type consistency: No code types or runtime interfaces are introduced. Terminology matches the approved spec: `scene alignment`, `controls`, no standalone `safety` folder.

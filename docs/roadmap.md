# Roadmap

## Product Intent

Advanced Show Control is a Tauri/Rust/React desktop app for Waves eMotion LV1 and LV1 Classic scene workflows. LV1 remains the source of truth for scene creation, scene recall, routing, plugins, mutes, processing, and live mixer state. The app is a fader-fade overlay: it stores fade metadata for LV1 scenes and moves only the scoped faders that the engineer has configured.

The app owns app-managed scene fade behavior, scoped channel targets, fade duration, fade execution, safety behavior, and show-file storage. It does not replace LV1 scenes or inspect LV1's internal scene scope.

## Safety Model

- Do not send fader commands when LV1 is disconnected, stale, unavailable, or unsafe.
- Do not bypass lockout checks, exact scene identity validation, or generation guards.
- Scene recall automation must validate before changing active fade ownership.
- Blocked, skipped, disabled, or unsafe recalls must not abort an existing fade.
- Recall fades must start from current live values, not stored capture-time starts.
- Manual override, Abort All, overlap/same-scene behavior, and disconnect safety must remain visible and test-covered.
- Faders should be scoped out of LV1 scenes for app-managed fades so LV1 does not jump them before the app can fade them.

## Completed Foundation

- Rust core crate, Tauri shell crate, and React/TypeScript frontend workspace exist.
- LV1 discovery, TCP connection, MyFOH-style handshake, keepalive, message logging, fader set commands, and basic hardware protocol findings are implemented.
- `Lv1Actor` mirrors connection state, current scene, scene list, channel topology, fader values, mute values, reconnect behavior, and snapshots.
- `FadeEngine` supports timed fades, curves, measured fader law, 25 Hz scheduling, minimum send delta, exact final sends, abort, scene-owned overlap behavior, same-scene repeat handling, and manual override detection.
- `ShowState` owns show data, selected scene editing, stored channel configs, scoped channel lists, scene toggles, duration, dirty state, and JSON `.lv1show` save/load.
- The Tauri shell exposes a working test-bed UI with connection, scene, and logs tabs, global lockout/abort controls, show-file controls, store workflow, duration editing, and scoped-channel controls.
- `SceneRecallFader` validates LV1 scene recall events, blocks unsafe recalls, skips disabled fader scope, starts validated scene-owned fades, and moves duration `0` scenes immediately.
- Runtime architecture is actor-oriented with `Lv1Actor`, `FadeEngine`, `ShowState`, `SceneRecallFader`, `ShellState`, `AppEventBus`, and `AppCommandBus` as the main ownership boundaries.

## MVP Roadmap

The immediate goal is to reach a live-viable MVP. This scope is intentionally larger than a prototype because the app is not useful in a live setting unless session handling, safety visibility, logging, frontend structure, and bundling are all trustworthy enough for rehearsal use. Historical phase planning is retired; this ordered list is the current build path.

1. Replace custom diagnostics logging with `tracing`.
   - Use more debug-level logging for log files.
   - Send info-level operational logging to the frontend as well.
   - Preserve user-visible safety blocks and operational events.
2. Make shell state projection more efficient.
   - Limit how often runtime state updates are sent to the frontend.
   - Keep safety-critical changes visible without flooding the UI.
3. Reduce false-positive manual override reports for balance/rotation fades.
   - Revisit pan-family override ownership, possibly limiting override authority to pan control instead of pan, balance, and width together.
   - Preserve clear manual override logs and cancellation behavior for safety-relevant moves.
4. Add show-file scene reconciliation/remapping.
   - Handle loaded show files whose stored scene references no longer match the current LV1 scene list.
   - Make mismatches and any skipped or unresolved mappings visible to the user.
5. Create a Storybook setup and start real frontend development.
   - Treat the current frontend as a test bed, not the final UI.
   - Establish component development outside the live Tauri runtime.
6. Set up frontend testing.
   - Add the test tooling needed for UI behavior and component coverage.
7. Build the frontend app shell.
   - Replace the test-bed layout with the real application frame.
   - Keep global safety controls prominent.
8. Build the frontend Scenes tab.
   - Support scene status, stored scene config review, scope editing, duration editing, and clear mismatch or safety warnings.
9. Build the frontend connection controls.
   - Support discovery, connect/disconnect, current LV1 status, and startup/reconnect clarity.
10. Build the frontend Settings tab.
   - Add the first app setting: auto-session recall.
   - Let engineers enable or disable automatic reload of the last session/show file when reconnecting to the same LV1 console.
   - Make the setting clear about safety behavior and when auto-recall will be skipped.
11. Build the frontend Sessions tab.
   - Support session management for app show/session files.
   - Provide clear import/export flows for engineers moving sessions between systems.
   - Surface save/load state, dirty state, current file location, and any import/export warnings.
   - Preserve manual session import/export as the explicit fallback path for automatic session handling.
12. Add auto-session recall.
   - Persist enough console identity metadata to avoid loading a session onto the wrong LV1 console.
   - Auto-reload the last session/show file only when the setting is enabled and the console identity matches safely.
   - Make skipped, blocked, successful, or failed auto-recall decisions visible in the UI and logs.
13. Build the frontend Logs tab.
   - Show frontend-facing info logs, safety blocks, recalls, fade starts, fade completions, manual overrides, and connection events.
14. Sort out bundling.
   - Produce a practical app package for MVP testing.
   - Document the packaging path and any platform limitations.

## MVP Exit Criteria

- A live engineer can connect to LV1, open or create a show file, store scoped fader targets for scenes, recall LV1 scenes, observe app-managed fades, abort safely, and understand the current app state without using a debug console.
- The UI is no longer a test bed and has clear app shell, Scenes, Sessions, Connection, Settings, and Logs areas.
- Engineers can manage sessions, including import/export, from the Sessions tab.
- Engineers can enable or disable auto-session recall from Settings.
- Auto-session recall safely reloads the last session/show file only when the saved console identity matches the current LV1 console.
- Manual session handling remains the fallback when auto-session recall is disabled, skipped, blocked, or fails.
- Logging is split appropriately between diagnostic files and frontend-facing operational events.
- Show-file scene mismatches can be reconciled or remapped without silently dropping app-managed fade configuration.
- Balance/rotation fades do not produce known false-positive manual override reports during normal timed fades.
- Frontend development has Storybook and test coverage in place for continued iteration.
- Shell state projection is bounded so routine LV1 updates do not overload the frontend.
- Bundling is good enough for MVP rehearsal/testing distribution.
- The release is hardened enough to be trusted in live-show workflows appropriate to its feature set; hardening is part of every release, not a separate final phase.

## Release 2: Cue Lists

After the scene-fading MVP, the next major release is cue list support. Cue lists let engineers build show-order lists from the LV1 scene library without changing the order of scenes on the console.

Core goals:

- Create, edit, save, and load cue lists as part of the app show file.
- Add LV1 scenes from the scene library to a cue list in any order.
- Allow the same LV1 scene to appear multiple times in the same cue list.
- Recall the selected cue through the app while preserving all existing scene recall and fade safety behavior.
- Support auto-next on recall so the selected cue list can advance automatically during a show.
- Make current cue, next cue, recall status, and blocked recall reasons clear in the UI.
- Create a documentation site for user-facing setup, operation, and troubleshooting docs.
- Document the MVP scene-fading workflow and the new cue list workflow.

Release 2 exit criteria:

- An engineer can build a show-order cue list from existing LV1 scenes without modifying the LV1 scene library order.
- The same scene can appear multiple times with each cue list entry treated as its own cue position.
- Recalling a cue triggers the correct LV1 scene and any app-managed fade behavior for that scene.
- Auto-next can be enabled so successful recall advances the cue list selection for show operation.
- Cue list recall cannot bypass lockout, scene identity validation, stale-state checks, generation guards, or existing fade safety rules.
- A documentation site exists with enough guidance for engineers to install, connect, manage sessions, configure scene fades, build cue lists, operate a show, abort safely, and troubleshoot common issues.
- The release is hardened enough to be trusted in live-show workflows appropriate to cue list operation.

## Release 3: Event Automation Engine

The third major release adds event automation. Engineers can create events with trigger conditions and actions; when the trigger conditions are met, the app fires the configured actions.

Core goals:

- Create, edit, enable, disable, save, and load event automations as part of the app show file.
- Define trigger conditions from app and LV1 state, such as scene recalls, connection state, fade state, cue list state, or other supported runtime events.
- Define one or more actions for each event, such as recalling scenes or cue list entries, starting app-managed fades, toggling lockout, aborting fades, or other supported app actions.
- Evaluate trigger conditions predictably and make fired, skipped, blocked, or failed actions visible in the UI and logs.
- Prevent automation loops or unsafe repeated firing.

Release 3 exit criteria:

- An engineer can build event automations without editing files by hand.
- Trigger conditions and actions are visible, reviewable, and testable before show use.
- Matching trigger conditions fire the intended actions.
- Safety checks still apply to every action; event automation cannot bypass lockout, scene identity validation, stale-state checks, generation guards, or fade safety rules.
- Automation activity is visible enough to troubleshoot why an event fired, skipped, or was blocked.
- The release is hardened enough to be trusted in live-show workflows appropriate to event automation.

## Release 4: External Control And Stream Deck

Release 4 adds a documented public integration API and a Stream Deck plugin. The public API is the integration foundation for external tools, including the Stream Deck plugin, while keeping LV1 communication centralized inside the app.

Core goals:

- Add a documented public HTTP API for app status, LV1 connection status, current scene, current cue, fade status, lockout state, and supported control actions.
- Add WebSocket events for live status, scene, cue, fade, warning, and automation updates.
- Route every external action through the same command and safety paths used by the UI.
- Make authentication, network exposure, and local-only defaults explicit before any public release.
- Build a Stream Deck plugin that talks to the app API, not directly to LV1.
- Support Stream Deck actions for recall, next cue, previous cue, abort all fades, lockout, and other safe app commands.
- Support Stream Deck feedback for LV1/app connection, current scene or cue state, fade running, lockout, manual override, and safety warnings.

Release 4 exit criteria:

- External clients can observe app state and trigger supported actions through documented endpoints.
- External control cannot bypass lockout, scene identity validation, stale-state checks, generation guards, or fade safety rules.
- Stream Deck can trigger and monitor supported app behavior through the plugin.
- API activity is logged clearly enough to diagnose external control problems.
- The release is hardened enough to be trusted in live-show workflows appropriate to external control and Stream Deck operation.

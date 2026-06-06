# LV1 Timed Fader Fade App: Project Phases

This document outlines a practical phased plan for building a timed fader fade overlay app for Waves eMotion LV1 / LV1 Classic scene workflows.

## Current Progress Checklist

- [x] **Phase 0: Product Definition And Risk Framing** — product model, safety defaults, MVP direction, and Rust/Tauri direction are established in `PROJECT.md` and design notes.
- [x] **Phase 1: LV1 Protocol Discovery Prototype** — discovery, TCP connection, MyFOH-style handshake, keepalive, message logging, fader set commands, rate testing, and hardware findings are implemented in the CLI/core.
- [x] **Phase 2: Core State Mirror** — `Lv1Actor` mirrors connection state, current scene, scene list, channel topology, fader values, mute values, events, reconnect behavior, and snapshots.
- [x] **Phase 3: Fade Engine Prototype** — fade engine, curves, measured fader law, 25 Hz scheduler, minimum send delta, final target send, abort, finish-now, replacement behavior, and manual override detection are implemented and tested.
- [ ] **Phase 4: Capture Engine And Listen Mode** — not implemented yet. Listen Mode state machine, capture threshold behavior, captured target table, accidental-touch removal, and save handoff remain to build.
- [ ] **Phase 5: Storage And Project Files** — not implemented yet. JSON project save/load, scene config validation, mismatch warnings, and backup/autosave remain to build.
- [~] **Phase 6: MVP Desktop UI** — partially implemented ahead of Phases 4–5. A durable Tauri + React + TypeScript + Tailwind shell exists with `Connection`, `Scene`, and `Logs` tabs, Rust-owned app snapshots, global lockout/abort/finish controls, and LV1 connection commands. Capture/save UI is still deferred.
- [ ] **Phase 7: Scene Recall Automation** — not implemented yet. Automatic fade triggering on LV1 scene recall, scene matching, safety blocks, and overlap policy remain to build.
- [ ] **Phase 8: HTTP And WebSocket Control API** — not implemented yet.
- [ ] **Phase 9: Bitfocus Companion Integration** — not implemented yet.
- [ ] **Phase 10: Beta Hardening** — not implemented yet.
- [ ] **Phase 11: Polished Release Candidate** — not implemented yet.

**Immediate Next Build Order:** finish Phase 4 capture workflow inside the new desktop shell, then Phase 5 storage, then Phase 7 recall automation.

---

## Phase 0: Product Definition And Risk Framing

**Goal:** Lock the app’s operating model before writing production code.

**Primary Outcome:** A short technical design brief that defines exactly what the app will and will not own.

**Scope:**

- Confirm that LV1 remains the source of truth for scene creation and scene recall.
- Define the app as a fader-fade overlay, not a scene manager.
- Define the app’s own fade scope as captured fader targets only.
- Document the operating rule that LV1 scene fader scope should be excluded for fade-enabled scenes.
- Decide the first supported platforms: likely macOS and Windows.
- Decide whether the first implementation is:
  - **Electron + TypeScript** for fastest protocol prototyping, or
  - **Rust + Tauri** for long-term reliability.

**Recommended Decision:** Start with a small TypeScript or Rust protocol prototype first, then decide whether the full product should be Tauri/Rust or Electron/TypeScript after the hardware tests.

**Exit Criteria:**

- MVP feature list is frozen.
- Data model is agreed.
- Safety behavior defaults are agreed.
- Hardware test plan is written.

---

## Phase 1: LV1 Protocol Discovery Prototype

**Goal:** Verify that the app can reliably observe and control the required LV1 state.

This is the most important phase. Do not build the full UI until this is proven.

**Build A Small Protocol Logger That Can:**

- Discover LV1 instances.
- Connect over the MyFOH-style OSC-over-TCP protocol.
- Complete handshake.
- Maintain ping/pong keepalive.
- Log all scene-related messages.
- Log all fader-related messages.
- Send test `/Set/Track/Out/Gain` commands to selected non-critical channels.

**Questions To Answer:**

- Does LV1 report current scene index and name on recall?
- Does LV1 Classic behave the same as software LV1?
- Are fader movement notifications sent for:
  - Physical surface faders?
  - On-screen fader moves?
  - Scene recall changes?
  - App-sent gain changes?
- Does LV1 echo changes back to the same client?
- What is the safe message rate for multiple simultaneous fader fades?
- Are channel, aux, group, matrix, LR, and DCA addressing consistent?
- Are indices definitely zero-based on the wire?

**Deliverables:**

- Protocol message log files.
- Confirmed message map.
- Confirmed fader address map.
- Safe update-rate recommendation.
- Known limitations document.

**Exit Criteria:**

- The app can read scene state.
- The app can read fader values.
- The app can set fader gain.
- The app can survive disconnect/reconnect without unsafe behavior.

---

## Phase 2: Core State Mirror

**Goal:** Build the internal live model of LV1 state.

**Components:**

- LV1 protocol client.
- Connection manager.
- Scene mirror.
- Channel topology mirror.
- Fader value mirror.
- Event bus for state changes.
- Logging layer.

**State To Mirror:**

- Connection status.
- Current scene index.
- Current scene name.
- Scene list, if available.
- Channel names.
- Channel groups and indices.
- Current fader values.
- Last notification timestamp.

**Important Behavior:**

The app should never assume stored state is current if the LV1 connection is unstable. If the connection drops, fades should stop or be prevented according to a clearly defined safety rule.

**Deliverables:**

- Headless app core.
- CLI or debug panel showing current LV1 state.
- Structured logs.
- Connection watchdog.

**Exit Criteria:**

- The app can run for an extended session while accurately mirroring LV1 state.
- State survives normal LV1 scene recalls.
- Disconnects are detected quickly and safely.

---

## Phase 3: Fade Engine Prototype

**Goal:** Implement reliable, cancelable, safety-aware fader fades.

**Core Features:**

- Fade from current live value to stored target value.
- 20–30 Hz update scheduler.
- Minimum send delta, such as 0.1 dB.
- Final exact target send.
- Fade curves:
  - Linear dB.
  - Ease-in-out dB.
  - Linear amplitude.
  - Ease-in-out amplitude.
- Abort all fades.
- Finish current fade immediately.
- Per-channel fade cancellation.
- Manual override detection.

**Recommended Defaults:**

| Setting | Default |
|---|---:|
| Fade Update Rate | 25 Hz |
| Default Fade Time | 4 Seconds |
| Minimum Send Delta | 0.1 dB |
| Manual Override Threshold | 0.5 dB |
| Default Curve | Ease-In-Out dB |
| Manual Override Mode | Touch Cancels Channel |

**Safety Logic:**

- If LV1 disconnects, stop sending immediately.
- If the connection is unstable, prevent new fade starts.
- If a fader reports a value that differs from the app’s expected value by more than the manual override threshold, cancel that channel’s fade.
- Always force-send the final target value only for channels still owned by the fade.
- Log every fade start, cancel, finish, abort, and manual override.

**Deliverables:**

- Fade engine library/module.
- Test harness with simulated faders.
- Real LV1 fade test on safe channels.
- Fade event logs.

**Exit Criteria:**

- Multiple faders can fade simultaneously.
- Manual override behavior works predictably.
- Abort and finish actions are reliable.
- The app does not continue sending after connection instability.

---

## Phase 4: Capture Engine And Listen Mode

**Goal:** Implement the preferred capture workflow.

**Workflow To Build:**

1. User selects or confirms the current LV1 scene.
2. User starts **Capture Faders For This Scene**.
3. App records the current fader state of all known channels.
4. App watches fader gain notifications.
5. Any fader moving beyond the capture threshold is added to the capture list.
6. App stores only the final target value for each moved fader.
7. User confirms, removes accidental touches, and saves.

**Capture Rules:**

- Capture should use a threshold of 0.2–0.5 dB.
- Captured targets should include:
  - Group.
  - Channel.
  - Channel name at capture.
  - Starting dB before capture.
  - Final target dB.
  - Delta.
  - Timestamp.
  - Enabled flag.
- Recall fades should always start from the current live value, not the capture start value.

**Deliverables:**

- Listen Mode state machine.
- Captured fader target table.
- Confirm/remove accidental touch behavior.
- Save fade config for current LV1 scene.

**Exit Criteria:**

- The engineer can capture faders without preselecting channels.
- Accidental touches can be removed before save.
- Saved target values match final fader positions.
- Capture works across the channel types selected for MVP.

---

## Phase 5: Storage And Project Files

**Goal:** Persist fade configurations safely and transparently.

**Recommended Storage Approach For MVP:**

Use a human-readable project file first, such as JSON. Move to SQLite later if the project grows.

**Stored Data:**

- App version.
- LV1 system/profile identifier, if available.
- Scene fade configs.
- Scene index.
- Scene name.
- Enabled state.
- Duration.
- Curve.
- Captured fader targets.
- Safety preferences.
- Last modified timestamp.

**Important Matching Behavior:**

Because LV1 scene indices and names may change, the app should warn when the current scene does not clearly match the saved config.

**Suggested Recall Matching Modes:**

| Match Case | Behavior |
|---|---|
| Index And Name Match | Run normally |
| Index Matches, Name Differs | Warn or block, depending on safety setting |
| Name Matches, Index Differs | Offer remap or allow with warning |
| Neither Matches | Do not auto-run |
| Duplicate Scene Names | Prefer index and warn |

**Deliverables:**

- Save/load project files.
- Import/export.
- Scene config validation.
- Mismatch warnings.
- Basic backup or autosave strategy.

**Exit Criteria:**

- Configurations survive app restart.
- Scene mismatch conditions are visible and safe.
- User can export and move a project file.

---

## Phase 6: MVP Desktop UI

**Goal:** Build a usable engineer-facing application.

**Main Screens:**

1. **Connection Screen**
   - LV1 discovered devices.
   - Connection status.
   - Current scene.
   - Current sample/session info if available.

2. **Current Scene Screen**
   - Current LV1 scene index/name.
   - Fade enabled status.
   - Fade time.
   - Fade curve.
   - Captured target list.
   - Capture button.
   - Save button.

3. **Capture Confirmation Screen**
   - Include checkbox.
   - Channel name.
   - Before value.
   - Target value.
   - Delta.
   - Remove accidental touch.
   - Confirm capture.

4. **Fade Status Screen**
   - Current fade running/not running.
   - Per-channel progress.
   - Manual override indicators.
   - Abort all.
   - Finish now.
   - Lockout toggle.

5. **Logs Screen**
   - Scene recalls.
   - Fade starts.
   - Fade completions.
   - Manual overrides.
   - Connection events.
   - Safety blocks.

**MVP UI Priorities:**

- Clarity over visual polish.
- Large status indicators.
- Large Abort All button.
- Minimal hidden behavior.
- Strong warnings for scene mismatch and unstable connection.

**Exit Criteria:**

- A live engineer can complete the full setup and recall workflow without using a debug console.
- All safety actions are accessible from the main screen.
- Current scene and fade status are obvious at a glance.

---

## Phase 7: Scene Recall Automation

**Goal:** Make fade-enabled scenes respond automatically to LV1 scene recalls.

**Behavior:**

When LV1 scene recall is detected:

1. Identify current scene index/name.
2. Check whether a fade config exists and is enabled.
3. Validate scene match.
4. Check lockout mode.
5. Check connection stability.
6. Read current live fader values.
7. Start fade for enabled captured targets.
8. Send progress/status events to UI and API.
9. Log completion or cancellation.

**Safety Blocks:**

Do not auto-fade if:

- LV1 connection is unstable.
- Scene identity is ambiguous.
- Lockout is enabled.
- Another fade is running and overlap is disallowed.
- No current fader values are available.
- Target channel topology no longer matches safely.

**Overlap Policy To Decide:**

Recommended MVP policy: recalling a new fade-enabled scene while a fade is running should cancel the previous fade and start the new one only after validation.

**Exit Criteria:**

- Normal LV1 scene recall triggers the correct fade.
- Non-fade-enabled scenes do not trigger fades.
- Ambiguous scene matches do not trigger unsafe behavior.
- Fade status remains clear.

---

## Phase 8: HTTP And WebSocket Control API

**Goal:** Support Companion, Stream Deck workflows, and external control.

**API Responsibilities:**

- Expose app status.
- Expose LV1 connection status.
- Expose current scene.
- Expose whether current scene has fade enabled.
- Trigger fade actions.
- Abort all fades.
- Finish current fade.
- Toggle lockout.
- Toggle fade enable for current scene.
- Emit live events over WebSocket.

**Suggested MVP Endpoints:**

| Method | Endpoint | Purpose |
|---|---|---|
| GET | `/api/status` | App, LV1, fade, and lockout status |
| GET | `/api/current-scene` | Current LV1 scene info |
| POST | `/api/fades/current/recall` | Recall fade for current scene |
| POST | `/api/fades/{id}/recall` | Recall specific fade config |
| POST | `/api/fades/abort` | Abort all fades |
| POST | `/api/fades/finish` | Finish current fade immediately |
| POST | `/api/lockout/toggle` | Toggle lockout |
| POST | `/api/current-scene/fade-enabled/toggle` | Toggle fade enable |
| WS | `/ws/events` | Status, scene, fade, and warning events |

**Exit Criteria:**

- Companion can control the app through HTTP.
- Companion can display status through polling or WebSocket events.
- External control cannot bypass safety checks.

---

## Phase 9: Bitfocus Companion Integration

**Goal:** Make Stream Deck control practical without building a native Stream Deck plugin first.

**Companion Actions:**

- Recall fade for current LV1 scene.
- Recall specific fade config.
- Abort all fades.
- Finish current fade immediately.
- Toggle fade enable for current scene.
- Toggle lockout mode.
- Next/previous app snapshot or fade config.

**Companion Feedbacks:**

- LV1 connected.
- App connected.
- Fade running.
- Current scene has fade enabled.
- Lockout enabled.
- Manual override detected.
- Scene mismatch warning.
- Connection unstable.

**Recommended Approach:**

Create a small Companion module that talks only to the app’s HTTP/WebSocket API. Avoid having Companion talk directly to LV1, so there is only one LV1 protocol client controlling faders.

**Exit Criteria:**

- Stream Deck can safely trigger and monitor fades.
- Abort All and Lockout are available from hardware buttons.
- Feedback state updates reliably.

---

## Phase 10: Beta Hardening

**Goal:** Make the app safe enough for controlled real-world rehearsal use.

**Testing Areas:**

- Long session stability.
- LV1 reconnect behavior.
- Scene list changes.
- Channel rename behavior.
- Channel topology changes.
- High channel-count fades.
- Rapid scene recalls.
- Manual override during fades.
- Companion control during fades.
- Lockout behavior.
- App restart during LV1 session.
- Network interruption.
- LV1 restart.
- Duplicate scene names.

**Additions:**

- Crash-safe project saving.
- More detailed logs.
- Diagnostics export.
- Versioned project file migration.
- Safe mode startup.
- “Do Not Send To LV1” dry-run mode.
- Warning banner for unverified topology.

**Exit Criteria:**

- The app behaves predictably in rehearsal scenarios.
- All known unsafe states block sending.
- Logs are sufficient to diagnose problems.

---

## Phase 11: Polished Release Candidate

**Goal:** Prepare the first public or private production-ready release.

**Polish Work:**

- Installer/package for macOS and Windows.
- Code signing if required.
- Clear onboarding flow.
- First-run safety checklist.
- Project templates.
- User manual.
- Companion setup guide.
- Hardware verification matrix.
- Release notes.
- Known limitations page.

**Documentation Should Emphasize:**

- LV1 scenes still own normal scene recall.
- Faders should be scoped out of LV1 scenes for fade-enabled scenes.
- The app owns only captured fader movement.
- Manual override behavior.
- Emergency abort workflow.
- Scene index/name mismatch behavior.
- Recommended update-rate and fade-time limits.

**Exit Criteria:**

- A new engineer can install, connect, capture, save, recall, abort, and troubleshoot using documentation alone.
- The release has been tested on the target LV1 variants.

---

# Suggested Timeline By Milestone

| Milestone | Phases | Result |
|---|---:|---|
| Technical Feasibility | 0–1 | Confirm LV1 protocol viability |
| Headless Core | 2–3 | Mirror LV1 and fade faders safely |
| Workflow MVP | 4–7 | Capture and auto-fade scenes |
| External Control | 8–9 | Companion and Stream Deck support |
| Production Hardening | 10–11 | Rehearsal-ready and release-ready app |

---

# Recommended MVP Cut Line

For the first useful version, include only:

- LV1 connection.
- Scene index/name mirror.
- Channel names and fader value mirror.
- Listen Mode capture.
- Confirmation table.
- Fade duration.
- Ease-in-out dB and linear dB curves.
- Scene recall detection.
- Fade captured faders from current live values.
- Touch Cancels Channel.
- Abort All.
- Finish Now.
- Lockout.
- Logs.
- JSON project save/load.

Defer:

- Native Stream Deck plugin.
- Advanced curve editor.
- SQLite.
- Cloud sync.
- Multi-console support.
- Deep scene-scope inspection.
- Complex remapping tools.
- DCA and matrix support until addressing is verified.

---

# Highest-Risk Items To Resolve First

The project should begin by resolving these before any major UI investment:

1. Whether scene recall notifications are reliable.
2. Whether fader notifications cover physical, on-screen, and scene-driven changes.
3. Whether app-sent fader commands are echoed back.
4. Whether simultaneous fader message rates are safe.
5. Whether LV1 Classic and computer-based LV1 behave the same.
6. Whether all desired fader types use stable group/channel addressing.
7. Whether scene index/name matching is reliable enough for automatic fades.

The core product is feasible as designed, but its success depends on validating the LV1 protocol behavior early. The safest build order is: **protocol logger first, fade engine second, capture workflow third, polished UI last.**

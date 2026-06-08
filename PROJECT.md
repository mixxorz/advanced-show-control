# LV1 Scene Fade utility

## Purpose

The proposed app adds **timed fader fades** to Waves eMotion LV1 / LV1 Classic scene workflows.

LV1 scenes already handle scene storage, recall, plugin changes, routing, Waves Tune key changes, mute states, and other console state. The missing feature is **snapshot fade time**: LV1 does not currently provide a native way to crossfade fader levels over time when recalling scenes.

This app acts as a **fade overlay** for LV1 scenes. It does not replace LV1 scenes. It attaches fade metadata and target fader values to LV1 scenes, then animates selected faders when those scenes are recalled.

---

## Core Concept

The app should treat LV1 as the source of truth for scene creation and scene recall.

The app owns only:

- Which LV1 scenes have fade behavior enabled.
- Which faders are included in each fade-enabled scene.
- The target fader value for each included fader.
- Fade duration.
- Fade curve.
- Safety behavior during fades.

The app watches LV1 scene recalls and fader changes through the reverse-engineered MyFOH-style OSC-over-TCP protocol.

---

## Preferred User Workflow

### Scene Setup Workflow

1. The engineer creates or recalls a normal LV1 scene.
2. In LV1, the engineer scopes out faders from the scene recall.
3. In the app, the engineer chooses **Capture Faders For This Scene**.
4. The app enters **Listen Mode**.
5. The engineer moves the faders they want included in the fade to their desired target positions.
6. The app records the faders that changed, along with channel names and target dB values.
7. The engineer confirms the captured channels and values.
8. The engineer sets fade time and curve.
9. The engineer saves the fade configuration for the current LV1 scene.

### Scene Recall Workflow

1. The engineer recalls an LV1 scene normally.
2. The app detects the current LV1 scene index/name.
3. If that scene has fade behavior enabled, the app reads the current fader values.
4. The app fades only the captured/scoped faders from their current live positions to the stored target values.
5. LV1 handles all other scoped scene changes instantly.

---

## Important Operating Rule

For fade-enabled scenes, faders should be excluded from LV1’s own scene scope.

If faders are included in the LV1 scene scope, LV1 may instantly jump them to their stored values before the app can fade them. The intended design is:

- LV1 scene recall handles non-fader parameters.
- The app handles fader motion over time.

This keeps the workflow simple and predictable.

---

## Capture By Listen Mode

The app should not require the engineer to manually select channels first. Instead, it should infer the app’s fade scope by watching which faders move during capture.

### Listen Mode Behavior

When Listen Mode starts, the app records the current fader state of all known channels. During Listen Mode, the app watches for fader gain notifications.

A fader is added to the capture list when its value changes beyond a defined threshold, such as 0.2 dB or 0.5 dB.

The app should save only the final target value for each moved fader.

Example:

```text
Initial value: -12.0 dB
Moved to:       -8.0 dB
Moved again to: -6.5 dB
Saved target:   -6.5 dB
```

### Suggested Capture Table

| Include | Channel    |   Before |   Target |   Delta |
| ------- | ---------- | -------: | -------: | ------: |
| Yes     | Lead Vocal |  -7.5 dB |  -3.0 dB | +4.5 dB |
| Yes     | BGV 1      | -12.0 dB |  -8.0 dB | +4.0 dB |
| Yes     | Keys       | -18.0 dB | -13.5 dB | +4.5 dB |

The confirmation step lets the engineer remove accidental fader touches before saving.

---

## Data Model

A fade configuration should be linked to an LV1 scene by both index and name.

```ts
type SceneFadeConfig = {
  lv1SceneIndex: number;
  lv1SceneName: string;
  enabled: boolean;
  durationMs: number;
  curve: "linearDb" | "easeInOutDb" | "linearAmplitude" | "easeInOutAmplitude";
  targets: CapturedFaderTarget[];
};

type CapturedFaderTarget = {
  group: number;
  channel: number;
  channelNameAtCapture: string;
  startDbBeforeCapture: number;
  targetDb: number;
  enabled: boolean;
  changedAt: number;
};
```

At recall time, the app should not use `startDbBeforeCapture` as the fade start. It should always use the current live fader value as the starting point.

The stored start value is useful for review, debugging, and possible undo behavior, but the live console state should always determine the fade start.

---

## Protocol Basis

Public reverse-engineered work suggests that LV1/MyFOH uses a native OSC-over-TCP protocol with Waves-specific discovery, framing, handshake, and notifications.

The public Bitfocus Companion LV1 module is the most useful existing reference. It indicates support for:

- LV1 discovery.
- TCP connection and handshake.
- Ping/pong keepalive.
- Scene list and current scene state.
- Channel topology and names.
- Fader state notifications.
- Fader set commands.
- Stream Deck integration through Companion.

### Important Fader Command

The relevant fader command appears to be:

```text
/Set/Track/Out/Gain
```

With arguments:

```text
i:<group>
i:<channel>
d:<gainDb>
```

Group and channel indices appear to be zero-based on the wire.

### Relevant Notification

The app should watch for fader gain notifications, likely:

```text
/Notify/Track/Out/Gain
```

The app should also watch scene notifications, such as current scene index/name and scene list updates.

---

## Scene Scope Limitation

The app probably cannot reliably read LV1’s internal scene scope from the currently known OSC/MyFOH protocol.

Known or likely available over OSC:

| Data                                      |                      Available |
| ----------------------------------------- | -----------------------------: |
| Current scene index/name                  |                            Yes |
| Scene list/names                          |                            Yes |
| Channel names/topology                    |                            Yes |
| Current fader values                      |                            Yes |
| Fader changes after scene recall          |                         Likely |
| LV1 scene scope/filter/recall-safe matrix | Unknown / probably not exposed |

Therefore, the app should maintain its own independent fade scope.

The app’s scope is simply the list of faders captured during Listen Mode.

---

## Fade Engine

When a fade-enabled LV1 scene is recalled, the app should:

1. Read the current fader values for all captured targets.
2. Compute interpolated gain values over the configured fade time.
3. Send repeated `/Set/Track/Out/Gain` messages until each target is reached.
4. Send the exact target value as the final send for each completed channel.

### Suggested Defaults

| Setting            | Suggested Default |
| ------------------ | ----------------: |
| Fade update rate   |          20–30 Hz |
| Default fade time  |       3–5 seconds |
| Minimum send delta |            0.1 dB |
| Capture threshold  |        0.2–0.5 dB |
| Default curve      |    Ease-in-out dB |

### Fade Curves

Recommended curve options:

| Curve                 | Use Case                               |
| --------------------- | -------------------------------------- |
| Linear dB             | Console-like predictable movement      |
| Ease-in-out dB        | Smooth musical scene transitions       |
| Linear amplitude      | More mathematically literal level ramp |
| Ease-in-out amplitude | Optional advanced behavior             |

A good default is **ease-in-out dB**.

---

## Manual Override Behavior

The app should handle manual engineer intervention during a fade.

Recommended modes:

| Mode                   | Behavior                                                               |
| ---------------------- | ---------------------------------------------------------------------- |
| Takeover               | App owns the fader until fade completes. Manual moves are overwritten. |
| Touch Cancels Channel  | Manual movement cancels only that channel’s fade.                      |
| Touch Cancels Snapshot | Any manual fader movement cancels the whole fade.                      |

Recommended default: **Touch Cancels Channel**.

Implementation idea:

- Track the last value sent by the app.
- If LV1 reports a fader value that differs from the expected value by more than a threshold, treat it as manual intervention.
- Cancel that channel’s fade.

Suggested threshold: around 0.5 dB.

---

## Stream Deck Support

The app should support Stream Deck indirectly at first, preferably through Bitfocus Companion.

Recommended architecture:

```text
Stream Deck
  ↓
Bitfocus Companion
  ↓
Your App HTTP/WebSocket API
  ↓
LV1 OSC-over-TCP
```

This keeps LV1 communication centralized inside the app. Stream Deck buttons should trigger the app, not talk directly to LV1.

### Companion Actions

Useful actions:

- Recall fade for current LV1 scene.
- Recall a specific fade config.
- Abort all fades.
- Toggle fade enable for current scene.
- Toggle lockout mode.
- Next/previous app snapshot.

### Companion Feedbacks

Useful feedbacks:

- LV1 connected.
- App connected.
- Fade currently running.
- Current scene has fade enabled.
- Lockout enabled.
- Manual override detected.

A native Stream Deck plugin could be added later, but Companion is the faster and more flexible first integration.

---

## Recommended Technology Stack

### Best Long-Term Option

```text
Rust core
Tauri desktop app
Web frontend, such as React, Svelte, Vue, or Solid
SQLite or JSON project files
HTTP/WebSocket API for Stream Deck/Companion
```

Rust is a good fit for:

- TCP protocol handling.
- OSC framing.
- Reliable scheduler/fade engine.
- Safety-critical live-sound behavior.
- Cross-platform packaging through Tauri.

The web UI is a good fit for:

- Snapshot tables.
- Scene management.
- Capture workflow.
- Fade curve editing.
- Stream Deck-style status displays.

### Fastest Prototype Option

```text
Electron + TypeScript
```

This may be faster because the existing Companion LV1 module is TypeScript-oriented, and OSC/WebSocket/Stream Deck tooling is very accessible in Node.js.

However, Electron is heavier than Tauri, and Rust may be preferable for a polished long-term tool.

---

## Suggested App Architecture

```text
LV1 Fade App
├── LV1 Protocol Client
│   ├── Discovery
│   ├── OSC-over-TCP framing
│   ├── Handshake
│   ├── Ping/pong keepalive
│   ├── Scene notifications
│   └── Fader notifications
│
├── State Mirror
│   ├── Scene list
│   ├── Current scene
│   ├── Channel names
│   ├── Fader values
│   └── Connection status
│
├── Capture Engine
│   ├── Listen Mode
│   ├── Change threshold
│   ├── Captured target list
│   └── Confirmation UI
│
├── Fade Engine
│   ├── Scheduler
│   ├── Curves
│   ├── Manual override detection
│   ├── Scene-owned overlapping fades
│   └── Abort all
│
├── Storage
│   ├── Scene fade configs
│   ├── App preferences
│   └── Import/export
│
└── Control API
    ├── Tauri commands
    ├── HTTP API
    └── WebSocket events
```

---

## Minimum Viable Product

The first useful version should include:

1. Connect to LV1.
2. Mirror current scene index/name.
3. Mirror channel names and fader values.
4. Enter Listen Mode for the current scene.
5. Capture moved faders as fade targets.
6. Save fade time and curve.
7. Detect LV1 scene recall.
8. Fade captured faders to stored targets.
9. Abort all fades.
10. Show connection and fade status.

---

## Important Safety Features

These should be included early:

- Global **Abort All Fades** button.
- Lockout mode to prevent accidental recalls.
- Manual override detection.
- Connection watchdog.
- No sending if LV1 connection is unstable.
- Exact final target send for completed owned channels.
- Logs of recalled scenes and fade actions.
- Clear warning if the current LV1 scene name/index no longer matches the saved fade config.

---

## Open Questions To Verify On Hardware

Before committing to a full build, verify these with a small protocol logger/prototype:

1. Does LV1 reliably send current scene index/name over OSC when a scene is recalled?
2. Does LV1 Classic behave the same as computer-based LV1 for these notifications?
3. Does LV1 send `/Notify/Track/Out/Gain` for physical fader movement?
4. Does LV1 send fader notifications when faders are moved on-screen?
5. Does LV1 send fader notifications after scene recall?
6. Does LV1 echo notifications back to the same OSC client that sent `/Set/Track/Out/Gain`?
7. What message rate is safe for simultaneous fader fades?
8. Are DCA faders addressed through the same track gain path?
9. Are LR, matrix, group, aux, and DCA group IDs consistent with the public Companion module?
10. How should the app behave if scene indices change but scene names remain the same?

---

## Product Summary

The app should be a **scene-aware fader fade overlay** for Waves LV1.

The intended operating model is:

```text
LV1 stores and recalls scenes.
The app watches the current LV1 scene.
The app captures fader targets by listening to moved faders.
The app stores fade metadata per LV1 scene.
When a fade-enabled scene is recalled, the app fades selected faders to their saved targets.
```

This design avoids needing access to LV1’s internal scene scope. It keeps the engineer’s normal LV1 workflow intact while adding the missing snapshot-fade behavior.

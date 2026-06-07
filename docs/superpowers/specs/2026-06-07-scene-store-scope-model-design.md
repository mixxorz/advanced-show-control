# Scene Store Scope Model Design

## Purpose

Replace Listen Mode capture with an explicit Store workflow. The engineer selects a scene, stores the current mixer state for that scene, then chooses which stored channels are in scope for applying the scene config.

The app is still under development, so no migration from the old `sceneFadeConfigs` / `fadeTargets` model is required.

## Data Model

Use broad scene and channel config names so the model can grow beyond fader values later.

```rust
pub struct SceneConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
}

pub struct ChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
}

pub struct ChannelRef {
    pub group: i32,
    pub channel: i32,
}
```

`duration_ms == 0` means this scene config is disabled for fader apply/fade behavior. Remove the separate `fade_enabled` flag.

`channel_configs` stores the scene's current per-channel values. For now it stores only `fader_db`, but it can later hold other channel-level state.

`scoped_channels` stores scene-level scope. It determines which channel configs are included when applying the scene config. Scope is not stored on each channel config.

Do not store channel names in scene configs or show files. Channel names are live display data only. A channel rename should not invalidate stored values for the same `(group, channel)` identity.

Do not store per-channel timestamps. There is no current use for per-channel history.

## Store Behavior

Add a Store command for the selected scene.

Store requires:

- A selected scene config.
- A loaded LV1 channel snapshot.

Store behavior:

- Read every current LV1 channel.
- Replace the selected scene's `channel_configs` with one `ChannelConfig` per current channel.
- Write each current `gain_db` as `fader_db: Some(gain_db)`.
- Preserve existing `scoped_channels` for channels that still exist in the newly stored channel set.
- If the selected scene had no prior channel configs, initialize `scoped_channels` to every stored channel.
- Mark the show file dirty.

Store is a fresh snapshot of current mixer state. It does not require Listen Mode and does not depend on fader movement events.

## Removed Workflow

Remove Listen Mode from the scene workflow.

Fader change events should continue to update the live LV1 mirror, but they should no longer mutate scene config state.

Remove capture-specific UI copy and actions, including instructions to start Listen Mode or move faders to create targets.

Decommission the old workflow completely:

- Remove `listen_mode_active` from app view state and internal shell state unless another non-capture use remains.
- Remove the `set_listen_mode` command and frontend call sites.
- Remove Listen Mode buttons, lockouts, and scene-selection restrictions.
- Remove fader-target capture logic from LV1 fader event handling.
- Remove target-level enable/remove commands and replace them with scene-level scope commands.
- Rename capture-era types and fields, including `SceneFadeConfig`, `FadeTarget`, `sceneFadeConfigs`, and `fadeTargets`, to the new scene config model.
- Replace capture-era tests with Store and scope tests rather than preserving old Listen Mode behavior.
- Update docs and user-facing text that describe captured targets or Listen Mode.

## UI Design

The Scene tab keeps scene selection on the left. The right pane edits the selected scene config.

Top controls:

- Selected scene title.
- Store button.
- Duration control where `0` means disabled.

Scope editor:

- If the scene has no `channel_configs`, show an empty state: "Store the current mixer state to choose scoped channels."
- Otherwise show All and None controls.
- All sets `scoped_channels` to every channel in `channel_configs`.
- None clears `scoped_channels`.
- Render channels as toggle buttons, not table checkboxes.
- Blue button means the channel is in `scoped_channels`.
- Grey button means the channel is out of scope.

Group the grid by display group:

- Inputs: LV1 group `0`.
- Groups: LV1 group `1`.
- Aux: LV1 group `2`.
- Matrix: LV1 group `6`.
- Masters: LV1 groups `3`, `4`, `5`, `7`, and `8`.
- Unknown: fallback for unrecognized groups.

Render groups in this order: Inputs, Groups, Aux, Matrix, Masters, Unknown.

Button labels:

- Inputs, Groups, Aux, and Matrix use the channel number.
- Masters uses fixed semantic labels because there is only one of each: `LR`, `C`, `Mono`, `Cue`, `TB`.

The UI may derive display grouping from numeric `group`, or Rust may expose helper labels. The underlying channel identity remains `(group, channel)`.

## Save And Load

Show files should serialize `sceneConfigs`, each with `channelConfigs` and `scopedChannels`.

Load validation should validate scenes against the current LV1 scene list. It should not remove channel configs or scoped channels because a channel name changed or because the current channel list does not contain an entry.

No migration from old show files is required.

## Testing

Add or update tests for:

- Store creates a full `channel_configs` snapshot from current LV1 channels.
- Store initializes `scoped_channels` to all channels on first store.
- Store preserves existing scope on later stores.
- Store prunes scope entries that no longer have a stored channel config after the fresh snapshot.
- Store fails when no scene is selected.
- Store fails when no LV1 channels are loaded.
- Fader events update the live mirror but do not mutate scene configs.
- Save/load uses the new `sceneConfigs`, `channelConfigs`, and `scopedChannels` shape.
- Load validation does not remove channel configs due to channel names or missing live channels.
- UI scope grid toggles individual channels and All/None.

# Waves eMotion LV1 OSC reference

This is a reverse-engineered OSC reference for Waves eMotion LV1. The main
sections describe known protocol behavior. Unconfirmed or version-dependent
behavior is listed under **Unconfirmed** sections.

## Disclaimer

This project is not affiliated with, endorsed by, or supported by Waves Audio Ltd.
or the Waves eMotion LV1 product team. Waves, eMotion, and LV1 are trademarks of
their respective owners. This software and documentation are provided without
warranty; use them at your own risk.

All group, channel, scene, aux, user-key, and mute-group indices are zero-based on
the OSC wire.

## Transport and framing

### Discovery

LV1 discovery uses a custom OSC multicast announcement.

| Field             | Value                                                                                             |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| Multicast address | `225.1.1.1:13337`                                                                                 |
| Message address   | `/zDNS`                                                                                           |
| Service type      | `_waveslv113._tcp`                                                                                |
| Packet contents   | service type, instance/session identifier, hostname, listening port, advertised host IP addresses |

LV1 can advertise multiple NICs, including loopback, APIPA, Docker/VM, or
host-only addresses.

### TCP framing

OSC messages over TCP are wrapped in LV1 framing:

```text
[4-byte big-endian OSC payload length][8-byte LV1 header][OSC payload]
```

| Field   | Value                     |
| ------- | ------------------------- |
| Length  | OSC payload length        |
| Header  | `00 00 00 02 00 00 00 00` |
| Payload | OSC packet                |

Multiple framed OSC messages may be concatenated in one TCP write.

### Handshake and keepalive

| Direction     | Address        | Arguments                  | Meaning                      |
| ------------- | -------------- | -------------------------- | ---------------------------- |
| Client to LV1 | `/handshake`   | `i:1 i:-1 i:1`             | MyFOH-style registration     |
| Client to LV1 | `/device_name` | `s:<device-name> s:<uuid>` | Client identity              |
| LV1 to client | `/handshake`   | `i:1`                      | Registration acknowledgement |
| LV1 to client | `/ping`        | varies                     | Keepalive ping               |
| Client to LV1 | `/pong`        | same as `/ping`            | Keepalive response           |

LV1 drops the connection after a short timeout without pong responses.

## OSC argument notation

This document uses compact OSC type prefixes:

| Prefix | Type              |
| ------ | ----------------- |
| `i`    | 32-bit integer    |
| `h`    | 64-bit integer    |
| `f`    | 32-bit float      |
| `d`    | 64-bit double     |
| `s`    | string            |
| `T`    | OSC boolean true  |
| `F`    | OSC boolean false |

Some notifications can arrive with either `f`, `d`, or `i` for numeric values.

## Channel topology

### `/Channels`

`/Channels` is the full LV1 channel snapshot. It arrives during connection startup
and during scene recall bursts before the current-scene notification.

| Address     | Arguments                              |
| ----------- | -------------------------------------- |
| `/Channels` | `i:<count> (<channel-record> x count)` |

Each channel record has 19 fields:

| Index | Type       | Name                         | Meaning                                                                                                              |
| ----- | ---------- | ---------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `0`   | `s`        | `name`                       | Channel display name                                                                                                 |
| `1`   | `i`        | `group`                      | LV1 group ID                                                                                                         |
| `2`   | `i`        | `channel`                    | Channel index within the group                                                                                       |
| `3`   | `d`        | `gain_db`                    | Output fader value in dB                                                                                             |
| `4`   | `d`        | `stereo_balance_or_sentinel` | Stereo balance/rotation initial value when `pan_mode == 2`; mono and non-pan channels can carry sentinel values here |
| `5`   | `i`        | unknown                      | Unknown                                                                                                              |
| `6`   | `i`        | unknown                      | Unknown                                                                                                              |
| `7`   | `i`        | unknown                      | Unknown                                                                                                              |
| `8`   | `i`        | unknown                      | Unknown                                                                                                              |
| `9`   | `i`        | unknown                      | Unknown                                                                                                              |
| `10`  | `i`        | unknown                      | Unknown                                                                                                              |
| `11`  | `i`        | unknown                      | Unknown                                                                                                              |
| `12`  | `i`        | unknown                      | Unknown                                                                                                              |
| `13`  | `i`        | unknown                      | Unknown                                                                                                              |
| `14`  | `i`        | unknown                      | Unknown                                                                                                              |
| `15`  | `i`        | unknown                      | Unknown                                                                                                              |
| `16`  | `i`        | `pan_mode`                   | `0` no pan controls, `1` mono pan, `2` stereo pan                                                                    |
| `17`  | `h`        | unknown                      | Unknown                                                                                                              |
| `18`  | `d` or `i` | `pan_degrees`                | Pan value in degrees                                                                                                 |

Active width values come from `/Notify/PanArcWidth`.

### Group IDs

| Group | Meaning                    | Typical channels                           |
| ----- | -------------------------- | ------------------------------------------ |
| `0`   | Input channels             | `Channel 1` through configured input count |
| `1`   | Mix groups                 | `Group 1` through `Group 8`                |
| `2`   | Aux / FX / monitor masters | `Fx 1` through `Fx 8`, `Mon 1` onward      |
| `3`   | LR master                  | singleton `LR`                             |
| `4`   | Center master              | singleton `Center`                         |
| `5`   | Mono master                | singleton `Mono`                           |
| `6`   | Matrix                     | `Matrix 1` through `Matrix 8`              |
| `7`   | Cue master                 | singleton `Cue`                            |
| `8`   | Talk Back master           | singleton `Talk Back`                      |
| `12`  | Link/DCAs                  | `Link 1` through `Link 16`                 |
| `24`  | Hidden link/control entry  | `HidLink:0`                                |

## Scenes

### Scene list

| Direction     | Address             | Arguments                                            | Meaning       |
| ------------- | ------------------- | ---------------------------------------------------- | ------------- |
| LV1 to client | `/Notify/SceneList` | `i:<count> (i:<scene-index> s:<scene-name>) x count` | Scene catalog |

The scene list is the catalog of LV1 scenes by index and name.

### Current scene name

| Direction     | Address              | Arguments        | Meaning            |
| ------------- | -------------------- | ---------------- | ------------------ |
| LV1 to client | `/Notify/Scene/Name` | `s:<scene-name>` | Current scene name |

### Current scene index

| Direction     | Address                 | Arguments         | Meaning             |
| ------------- | ----------------------- | ----------------- | ------------------- |
| LV1 to client | `/Notify/CurSceneIndex` | `i:<scene-index>` | Current scene index |

The current scene identity is the pair of current scene index and current scene
name.

### Recall command

| Direction     | Address              | Arguments         | Meaning               |
| ------------- | -------------------- | ----------------- | --------------------- |
| Client to LV1 | `/Set/CurSceneIndex` | `i:<scene-index>` | Recall scene by index |

### Scene edit caveat

LV1 can emit scene-list and current-scene notifications during scene list edits.
Cases include moving the current scene and renaming a non-current scene.

## Faders, mute, and solo

### Output fader

| Direction     | Address                  | Arguments                                     | Meaning            |
| ------------- | ------------------------ | --------------------------------------------- | ------------------ |
| LV1 to client | `/Notify/Track/Out/Gain` | `i:<group> i:<channel> f\|d\|i:<gain_db> ...` | Output fader value |
| Client to LV1 | `/Set/Track/Out/Gain`    | `i:<group> i:<channel> d:<gain_db>`           | Set output fader   |

Fader range is `-144.0` dB to `+10.0` dB. Fader-position interpolation is not
dB-linear.

### Output mute

| Direction     | Address                  | Arguments                             | Meaning           |
| ------------- | ------------------------ | ------------------------------------- | ----------------- |
| LV1 to client | `/Notify/Track/Out/Mute` | `i:<group> i:<channel> i:<muted> ...` | Output mute state |
| Client to LV1 | `/Set/Track/Out/Mute`    | `i:<group> i:<channel> T\|F`          | Set output mute   |

### Solo

| Direction     | Address         | Arguments                        | Meaning               |
| ------------- | --------------- | -------------------------------- | --------------------- |
| LV1 to client | `/Notify/Solo`  | `i:<group> i:<channel> i:<solo>` | Solo state            |
| Client to LV1 | `/Set/Solo`     | `i:<group> i:<channel> i:<solo>` | Set solo state        |
| Client to LV1 | `/ClearAllSolo` | none                             | Clear all solo states |

## Pan family

The pan family has three parameters when they are known and applicable: pan,
balance/rotation, and width.

### Pan

| Direction     | Address             | Arguments                                                   | Meaning   |
| ------------- | ------------------- | ----------------------------------------------------------- | --------- |
| LV1 to client | `/Notify/Track/Pan` | `i:<group> i:<channel> d:<pan_degrees> i:<active_or_valid>` | Pan value |
| Client to LV1 | `/Set/Track/Pan`    | `i:<group> i:<channel> d:<pan_degrees>`                     | Set pan   |

Range: `-45.0..=45.0` degrees.

### Balance / rotation

| Direction     | Address                  | Arguments                                                       | Meaning                |
| ------------- | ------------------------ | --------------------------------------------------------------- | ---------------------- |
| LV1 to client | `/Notify/Balance`        | `i:<group> i:<channel> d:<balance_degrees> i:<active_or_valid>` | Balance/rotation value |
| Client to LV1 | `/Set/Track/Pan/Balance` | `i:<group> i:<channel> d:<balance_degrees>`                     | Set balance/rotation   |

Range: `-45.0..=45.0` degrees.

### Width

| Direction     | Address                | Arguments                                    | Meaning     |
| ------------- | ---------------------- | -------------------------------------------- | ----------- |
| LV1 to client | `/Notify/PanArcWidth`  | `i:<group> i:<channel> d:<width> i:<active>` | Width value |
| Client to LV1 | `/Set/Track/Pan/Width` | `i:<group> i:<channel> d:<width>`            | Set width   |

Active width range: `-1.4..=1.4`. Stereo default: `1.0`. Active width `0.0` is a
collapsed-width value.

Active flag:

| Value | Meaning                          |
| ----- | -------------------------------- |
| `i:1` | Width value is active            |
| `i:0` | Width is inactive for this tuple |

Inactive values can be sentinel-like. Known examples include `857.1428571428571`
for many mono input/aux tuples and `0` for many master/control tuples.

### LR pan exception

LR is `group=3 channel=0`. It can report `pan_mode=2` while width is inactive and
does not have a pan knob.

## Aux tracks and sends

### Aux track names

| Direction     | Address           | Arguments                                          | Meaning                 |
| ------------- | ----------------- | -------------------------------------------------- | ----------------------- |
| LV1 to client | `/Aux/Tracks`     | `i:<count> (i:<index> i:<group> s:<name>) x count` | Aux track names         |
| Client to LV1 | `/Get/Aux/Tracks` | none                                               | Request aux track names |

Names include `Fx 1` through `Fx 8` and `Mon 1` onward, all with group `2`.

### Aux send on

| Direction     | Address               | Arguments                                                | Meaning               |
| ------------- | --------------------- | -------------------------------------------------------- | --------------------- |
| LV1 to client | `/Notify/Aux/Send/On` | `i:<source-group> i:<source-channel> i:<aux-index> T\|F` | Aux send on/off state |
| Client to LV1 | `/Set/Aux/Send/On`    | `i:<source-group> i:<source-channel> i:<aux-index> T\|F` | Set aux send on/off   |

For input-channel sends, `source-group` is normally `0`.

### Aux send gain

| Direction     | Address                 | Arguments                                                             | Meaning           |
| ------------- | ----------------------- | --------------------------------------------------------------------- | ----------------- |
| LV1 to client | `/Notify/Aux/Send/Gain` | `i:<source-group> i:<source-channel> i:<aux-index> f\|d\|i:<gain_db>` | Aux send gain     |
| Client to LV1 | `/Set/Aux/Send/Gain`    | `i:<source-group> i:<source-channel> i:<aux-index> d:<gain_db>`       | Set aux send gain |

### Aux send pan

| Direction     | Address             | Arguments                                                   | Meaning          |
| ------------- | ------------------- | ----------------------------------------------------------- | ---------------- |
| Client to LV1 | `/Set/Aux/Send/Pan` | `i:<source-group> i:<source-channel> i:<aux-index> d:<pan>` | Set aux send pan |

### Unconfirmed: Aux send pan notifications

The set command for aux send pan is known. Matching notification behavior and
value range are unconfirmed.

## Surface and utility messages

These messages are useful for surface integration and diagnostics.

### Layers

| Direction     | Address          | Arguments                                    | Meaning              |
| ------------- | ---------------- | -------------------------------------------- | -------------------- |
| LV1 to client | `/Notify/Layers` | `i:<page> i:<is_custom> i:<layer_count> ...` | Surface layer layout |

Layer payloads contain layer names and `(group, channel)` entries. Empty slots use
`group=-1, channel=-1`. This is surface layout data. `/Channels` is the full
channel snapshot.

### Current layer

| Direction     | Address                | Arguments                        | Meaning               |
| ------------- | ---------------------- | -------------------------------- | --------------------- |
| LV1 to client | `/Notify/CurrentLayer` | `i:<mixer_page> i:<layer_index>` | Current surface layer |

### User keys

| Direction     | Address               | Arguments                                                | Meaning             |
| ------------- | --------------------- | -------------------------------------------------------- | ------------------- |
| LV1 to client | `/Notify/UserKeyInfo` | `i:<key-index> s:<short-name> s:<function> i:<assigned>` | User key assignment |
| Client to LV1 | `/Set/UserKey`        | `i:<key-index> T\|F`                                     | Set user key state  |

### Spill buttons

| Direction     | Address               | Arguments                     | Meaning                |
| ------------- | --------------------- | ----------------------------- | ---------------------- |
| LV1 to client | `/Notify/SpillButton` | `i:<bank> i:<slot> i:<state>` | Spill button state     |
| Client to LV1 | `/Set/SpillButton`    | `i:<bank> i:<slot> i:<state>` | Set spill button state |

Spill expands a group/link/DCA-style target onto surface faders.

### Mute groups

| Direction     | Address             | Arguments                   | Meaning              |
| ------------- | ------------------- | --------------------------- | -------------------- |
| LV1 to client | `/Notify/MuteGroup` | `i:<mute-group-index> T\|F` | Mute group state     |
| Client to LV1 | `/Set/MuteGroup`    | `i:<mute-group-index> T\|F` | Set mute group state |

### Talkback / internal assign

| Direction     | Address                  | Arguments                                                    | Meaning                   |
| ------------- | ------------------------ | ------------------------------------------------------------ | ------------------------- |
| LV1 to client | `/Notify/InternalAssign` | `i:<group> i:<channel> i:<type> i:<sub> i:<state> i:<valid>` | Internal assignment state |

Known selectors:

| Type | Meaning                                                                                                  |
| ---- | -------------------------------------------------------------------------------------------------------- |
| `2`  | Talk Back destination toggle. `group=8 channel=0`; `sub` is the aux destination index; `state` is on/off |
| `7`  | Cue source / flip-related state                                                                          |

### Unconfirmed: Cue / flip internal assign

`type=7` is not fully mapped. The complete state model is unknown.

### Tempo

| Direction     | Address              | Arguments         | Meaning             |
| ------------- | -------------------- | ----------------- | ------------------- |
| LV1 to client | `/Notify/Tempo`      | `f:<bpm>`         | Tempo               |
| LV1 to client | `/Notify/TempoBlink` | `i:<state>`       | Tempo blink state   |
| Client to LV1 | `/TapTempo`          | `i:1`, then `i:0` | Momentary tap tempo |

## High-volume and diagnostic messages

| Address                     | Meaning                                                                 |
| --------------------------- | ----------------------------------------------------------------------- |
| `/Notify/Meters`            | High-volume meter batch                                                 |
| `/Notify/NetworkCongestion` | Diagnostic status                                                       |
| `/Notify/TempoBlink`        | Frequent tempo blink status                                             |
| `/Notify/ServiceVersion`    | LV1 version information                                                 |
| `/Notify/CurrentSessionID`  | Session identifier                                                      |
| `/Notify/TrackColor`        | Channel color metadata                                                  |
| `/Notify/Track/Name`        | Channel naming notification                                             |
| `/Notify/TrackName`         | Channel naming notification                                             |
| `/Notify/NumberOfChannels`  | Input-channel count candidate; `/Channels` provides the fuller topology |

## Unconfirmed

- Which groups are pan-family capable beyond the LR exception.
- What are the remaining `/Channels` fields beyond the known name, group, channel,
  gain, balance candidate, pan mode, and pan fields?
- Are all aux-send notification numeric types stable across LV1 versions, or do
  they vary like some fader notifications?

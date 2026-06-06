# Phase 1 Hardware Validation

Target: Waves eMotion LV1 software.

## Commands

### Discovery

Run:

```bash
cargo run -- discover --timeout-ms 6000
```

Record whether LV1 appears, which IP is selected, and which TCP port is advertised.

### Listen

Run:

```bash
cargo run -- listen --log-dir logs
```

While listening, recall scenes in LV1 and move safe test faders. Record which OSC addresses appear.

### Gain Send

Pick a non-critical channel. Run:

```bash
cargo run -- set-gain --group 0 --channel 0 --gain-db -20
```

Record whether LV1 moves the fader and whether any echo or notification appears.

## Results

- Discovery finds LV1: not run.
- Handshake succeeds: not run.
- Ping/pong keeps session alive: not run.
- Scene recall messages observed: not run.
- Fader movement messages observed: not run.
- App-sent gain command works: not run.
- App-sent gain echo behavior: not run.

## Notes

Add dated notes here after each hardware run.

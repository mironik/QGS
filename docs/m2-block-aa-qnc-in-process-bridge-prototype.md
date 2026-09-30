# M2 Block AA - QNC In-Process Bridge Prototype

Block AA adds `crates/qnc-qgs-bridge`.

There is no QNC application checkout in this workspace, so the prototype lives
here as the facade QNC controller code can call. It is not part of the QGS
player runtime.

## What It Does

`Bridge::open_session` takes a `qnc-qgs-contract` prepared input, rejects
path-shaped bindings, and sends `LoadPreparedInput` through
`QgsQncControlSurface`.

Later calls send prepare, cue, preroll, play, pause, seek, stop, unload, and
snapshot with the stored generation. The bridge keeps the latest accepted
generation. A snapshot does not advance it. A rejected command does not either.

Public snapshots expose source mode, picture, original-audio authority, active
range, and the device non-claims. They do not expose private binding refs.
`FramePresented` is not projected.

`play` does not prepare or open media. QGS rejects it until the runtime is
ready.

## What It Does Not Own

- QGS runtime internals beyond the control-surface call
- decode, display, or audio devices
- QNC UI, database, or workflow
- a second player state machine
- IPC

## Check

```bash
cargo test -q -p qnc-qgs-bridge
```

## Non-Claims

Block AA does not claim QNC UI integration, IPC, realtime playback, A/V sync,
real display, or production audio.

Evidence level: `InProcessBridgeEvidence`.

## Recommended Next Block

```text
M2 Block AB - QNC host record on qnc-qgs-bridge
```

See `docs/m2-block-ab-qnc-host-adopts-qnc-qgs-bridge.md`. A writable QNC
checkout is still required before the player controller can depend on this
crate. Do not copy the bridge into QGS playback code.

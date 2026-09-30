# M2 Block AC - Command Script Parity

Block AC drives the Block S command script through `qnc-qgs-bridge` and checks
the public snapshots. The entry is `Bridge::open_from_host` with the original
media host record. There is still no writable QNC checkout, so this stays an
in-process script.

## Script

Accepted steps, in order:

1. Load via `open_from_host`. Status is `Loaded`. The active range is not
   applied yet.
2. Prepare. Status stays `Loaded`. Active range becomes `10..90`.
3. Cue frame 10. Status becomes `Ready`. Playhead is frame 10.
4. Preroll at frame 10. Playhead stays at 10.
5. Play. Status becomes `Playing`.
6. `play_for(2)` advances the playhead to frame 12.
7. Pause holds frame 12.
8. Seek to frame 20.
9. Preroll again, then play. Seek clears readiness, so play before that
   preroll is rejected.
10. Stop. Source stays loaded and the active range stays `10..90`.
11. Unload. Status is `Empty` and the public source URI is cleared.

Rejected steps leave generation unchanged: play before prepare, cue at the
exclusive range end, and seek at the exclusive range end. A snapshot after
preroll does not advance generation.

Public snapshots keep the original media URI, `OriginalMedia`, original MXF
audio, and the device non-claims. `FramePresented` is absent. Ordered events
include `SourceLoaded`, `PreparedInputAccepted`, `Cued`, `PrerollReady`,
`Started`, `Ticked`, `Paused`, `Seeked`, `Stopped`, `Unloaded`, and
`CommandRejected`.

## Check

```bash
cargo test -q -p qnc-qgs-bridge --lib command_script_matches_public_snapshot_contract
```

## Non-Claims

Block AC does not claim a QNC controller dependency, passive QNC view
projection, IPC, realtime playback, A/V sync, real display, or production
audio.

Evidence level stays `InProcessBridgeEvidence`.

## Recommended Next Block

```text
M2 Block AD - passive projection mocks
```

See `docs/m2-block-ad-passive-projection-mocks.md`.

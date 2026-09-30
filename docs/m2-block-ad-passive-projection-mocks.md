# M2 Block AD - Passive Projection Mocks

Block AD projects a public bridge snapshot into monitor, timeline, wave, and
status facts. The shapes follow the QNC passive-view rules inspected in
`qnc-player-timeline`, `qnc-monitor`, and `qnc-wave`. Those crates stay in
QNC. This module does not import them, paint UI, or read a QNC database.

`Bridge::project_passive` is the entry.

## Projection Rules

- Timeline playhead is the confirmed carrier frame. Prepare and preroll can
  publish the half-open active range (`10..90` becomes start 10, duration 80)
  while the playhead is still empty. Cue becomes enabled only after that
  carrier frame exists and the loaded timebase is valid.
- Monitor picture requires `visual_verified`. A `FramePresented` event label
  does not turn the monitor into a picture. Until then the monitor shows the
  transport status, or empty after unload.
- Wave lists the loaded discrete mono lanes (`A1`–`A4` plus source track
  index) and the original-audio sample cursor. Peaks stay out. Proxy AAC is
  not the wave source.
- Status enables play only from `Ready` with a confirmed frame, and pause only
  while `Playing`. Device non-claims stay false. A rejected command keeps its
  reason on the status projection.

Unload clears the public URI, lanes, and playhead.

## Check

```bash
cargo test -q -p qnc-qgs-bridge --lib passive_projection
```

## Non-Claims

Block AD does not claim a QNC controller dependency, QNC widget integration,
IPC, realtime playback, A/V sync, real display, or production audio.

Evidence level stays `InProcessBridgeEvidence`.

## Recommended Next Block

```text
M2 Block AE - QNC operator actions
```

See `docs/m2-block-ae-qnc-operator-actions.md`.

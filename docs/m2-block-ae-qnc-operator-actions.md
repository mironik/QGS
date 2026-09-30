# M2 Block AE - QNC Operator Actions

There is still no writable QNC checkout. Block AE adds the operator action
surface from `qnc-player-client` onto `qnc-qgs-bridge`, so a later controller
can call it without a process socket.

`Bridge::apply_action` accepts:

- `TogglePlayPause` — pause while `Playing`, otherwise play
- `Step(delta)` — move the carrier frame and clamp it inside the half-open
  active range (`10..90` clamps to `10..=89`)
- `Cue(frame)` — cue that frame

Until the snapshot has a loaded source, a carrier frame, an active range, and
a valid timebase, the action is rejected with `Player has no confirmed
position.` The runtime is not called, and generation stays put.

QNC's wire cue carries `present_frame: true`. That flag does not set
`visual_verified`. The passive monitor stays on the transport status.

## Check

```bash
cargo test -q -p qnc-qgs-bridge --lib operator_actions_wait_for_a_confirmed_playhead_and_keep_picture_empty
```

## Non-Claims

Block AE does not claim a QNC controller dependency, IPC, realtime playback,
A/V sync, real display, or production audio.

Evidence level stays `InProcessBridgeEvidence`.

## Recommended Next Block

```text
M2 Block AF - QNC player-client view
```

See `docs/m2-block-af-qnc-player-client-view.md`.

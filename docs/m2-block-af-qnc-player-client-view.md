# M2 Block AF - QNC Player-Client View

There is still no writable QNC checkout. Block AF adds the predicates
`qnc-player-client::View` uses, so a later client can read them from
`qnc-qgs-bridge` instead of a player-process event envelope.

`Bridge::player_view` reports:

- `has_confirmed_position` after a loaded source, carrier frame, active range,
  and valid timebase exist
- `ready` and `can_start_playback` only when that position is also
  `transport_ready`
- `playing` from the transport status
- `source_frame_interval` from the loaded timebase once the position is
  confirmed (25/1 is 40 ms)
- `video_visible` stays false, including while playing
- `preparing` stays false after `open_from_host`, because that call finishes
  before it returns

A step clears transport readiness, so playback cannot start again until a
later cue or preroll restores it. The frame interval remains, because the
carrier frame is still confirmed.

## Check

```bash
cargo test -q -p qnc-qgs-bridge --lib player_view_matches_qnc_client_predicates_without_video
```

## Non-Claims

Block AF does not claim a QNC controller dependency, GPU/DMA monitor frames,
IPC, realtime playback, A/V sync, real display, or production audio.

Evidence level stays `InProcessBridgeEvidence`.

## Recommended Next Block

When a writable QNC checkout exists, `qnc-player-client` should open this
bridge, send `OperatorAction`s, and read `player_view` plus `project_passive`
for the existing timeline, monitor, and wave views. Do not start IPC or
Wayland before that dependency exists.

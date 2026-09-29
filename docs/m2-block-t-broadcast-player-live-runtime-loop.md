# M2 Block T - Broadcast Player Live Runtime Loop

Block T adds an operator-facing live backend loop for the QGS Broadcast Player
Runtime.

This is not a new device backend and not a realtime certification block. It
does not implement Wayland/Vulkan, X11, DRM/KMS, production PipeWire audio, QNC
UI, IPC, export/render, realtime playback verification, A/V sync certification,
or real display output.

## Why This Block Exists

Recent M2 work proved many runtime surfaces, but most commands were bounded
proofs or reports. Block T makes the backend player feel live: it starts the
existing Broadcast Player runtime, keeps it ticking, and shows playhead, media
time, audio sample range, prepared window, and buffer status as the loop runs.

The loop is still backend/logical. It is intended for operator/developer
visibility, not for final device output.

## Difference From Bounded Operator Run

Existing command:

```bash
--qgs-broadcast-player-run <original-mxf> <proxy-mp4>
```

This remains a scripted report:

- load
- prepare
- cue
- bounded play
- pause
- seek
- bounded replay
- stop
- unload

New command:

```bash
--qgs-broadcast-player-live <original-mxf> <proxy-mp4>
```

This starts a live-ticking loop and keeps advancing until max frames or
end-of-range. It is closer to running the backend player, but still does not
claim real display, production audio, realtime verification, or A/V sync.

## Command Syntax

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <original-mxf> <proxy-mp4> \
  --start-frame <frame> \
  --status-every <n> \
  --max-frames <n> \
  --seek-frame <frame> \
  --view compact \
  --pace logical \
  --no-interactive
```

Options:

| Option | Default | Meaning |
| --- | --- | --- |
| `--start-frame <frame>` | `0` | Initial cue frame. Clamped to the active range. |
| `--status-every <n>` | `10` | Print live status every `n` logical ticks. |
| `--max-frames <n>` | `500` | Safety cap for live ticks. The loop also stops at end-of-range. |
| `--seek-frame <frame>` | none | Optional scripted seek after the loop has begun. |
| `--view compact|detailed` | `compact` | Operator compact output or richer diagnostic output. |
| `--pace logical|wall` | `logical` | Logical loop or approximate wall-clock preview sleep. |
| `--no-interactive` | false | Disable stdin controls. Block T introduced the live loop; Block U adds interactive stdin control input. |

## Block U Interactive Control Layer

Block U layers simple stdin controls on top of the same live command. The
runtime chain remains:

```text
QgsBroadcastPlayerAssembly
  -> QgsQncControlSurface
  -> QgsBroadcastPlayerOperationalRuntime
```

Available controls:

- `p` / `pause`
- `r` / `resume` / `play`
- `s <frame>` / `seek <frame>`
- `status`
- `stop`
- `q` / `quit`
- `help`

`status` requests a non-mutating snapshot through the control surface. Runtime
mutations still go through QNC-shaped command envelopes; `qgs-test` only parses
operator input and prints results.

## Live Loop Behavior

The command:

1. Builds `QgsBroadcastPlayerAssembly` from the existing path-based acceptance
   input.
2. Creates `QgsQncControlSurface`.
3. Sends `LoadPreparedInput`.
4. Sends `Prepare`.
5. Sends `Cue` at the selected start frame.
6. Sends `Preroll` at the selected start frame.
7. Sends `Play`.
8. Repeatedly sends one-frame `Play { frame_count: Some(1) }` commands through
   the control surface.
9. Prints status periodically.
10. Stops at end-of-range, rejection, or `--max-frames`.
11. Sends `Stop`.
12. Sends `Unload`.

The loop does not duplicate the player state machine. Runtime behavior comes
from the existing assembly, operational runtime, and QNC-shaped control
surface.

## Pace Modes

`logical` is the default. The loop advances as quickly as command execution and
output allow.

`wall` sleeps for the source frame duration between ticks. This is only a
wall-clock paced preview loop. It is not `RealtimeVerified`.

## Media Time Display

Each status line prints media time from the runtime snapshot:

```text
tick=000010 frame=10   t=00:00:00.200 audio=[9600..10560) window=[7..15) buffer=ready
```

For the 50 fps sample, frame 10 maps to `00:00:00.200` and frame 250 maps to
`00:00:05.000`.

## Prepared Window / Buffer Display

Compact status includes:

- current frame
- media time
- original-audio sample range
- prepared frame window
- buffer status

The loop reports existing prepared-window facts. It does not fake decode,
device readiness, display presentation, or audio device verification.

## Stop / Completion Policy

The live loop stops when:

- the active range reaches its end.
- `--max-frames` is reached.
- a runtime/control command is rejected.

The default `--max-frames 500` is intentionally conservative so an omitted cap
does not create an accidental long-running process on long clips.

## Control Surface / Runtime Layer Used

Block T drives:

```text
QgsBroadcastPlayerAssembly
  -> QgsQncControlSurface
  -> QgsBroadcastPlayerOperationalRuntime
```

This keeps generation checks, command replies, snapshots, events, and private
path rules on the same QNC-shaped surface already introduced by Block R.

## Sample-002 Result

Command used:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  /home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik\ 1560.MXF \
  /home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik\ 1560S03.MP4 \
  --max-frames 80 --status-every 10 --view compact --no-interactive
```

Expected behavior:

- mode: `ProxyPreview`
- video: proxy MP4
- audio: original MXF mono lanes
- proxy AAC authoritative: no
- live ticks advance frame/audio position
- media time is printed
- prepared window and buffer state are printed
- run stops cleanly at max frames or source end
- private path exposed: no
- real display: `NotImplemented`
- realtime verified: no
- A/V sync verified: no
- production audio verified: no
- real `FramePresented`: no

## Mironik 2002 Result

Suggested command if the long-form media is available:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <mironik-2002-original-mxf> <mironik-2002-proxy-mp4> \
  --max-frames 500 --status-every 25 --view compact --no-interactive
```

The same non-claims apply. This command is useful for observing sustained
logical playhead progression on a longer source.

## Non-Claims

Every live run preserves these boundaries:

- real display: `NotImplemented`
- QNC OS display target: Wayland + Vulkan
- X11: legacy/non-target only
- visual verified: no
- realtime verified: no
- production audio verified: no
- A/V sync verified: no
- real `FramePresented`: no
- production PipeWire audio: no
- QNC UI integration: no
- IPC: no
- export/render: no

## Recommended Next Block

Recommended next milestone:

```text
M2 Block U - Broadcast Player Live Control Input
```

Block U is now implemented as the stdin control layer described above.

Goal:

- add simple stdin/operator controls for the live loop.
- support pause/resume, seek, status, and quit.
- preserve the same QGS control surface and truth boundaries.
- avoid real device output, IPC, QNC UI, realtime certification, and A/V sync
  certification.

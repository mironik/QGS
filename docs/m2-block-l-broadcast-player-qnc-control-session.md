# M2 Block L — Broadcast Player QNC Control Session

Block L adds an in-process QNC-style control session for the existing QGS
Broadcast Player Runtime.

This is not IPC, not a QNC UI, not an API freeze, and not a new playback
backend. `qgs-test` parses a small command script, sends commands to the
existing `QgsBroadcastPlayerOperationalRuntime`, and prints the resulting
runtime snapshot after each command.

## Why This Exists

`--qgs-broadcast-player-run` shows the player working through a fixed operator
scenario. The control session shows the next shape: a QNC-style controller
issuing explicit commands to the backend player and observing state after each
command.

The goal is to make the backend feel controllable without introducing a second
state machine or pretending that device backends exist.

## Command

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-control-session <original-mxf> <proxy-mp4>
```

Optional script:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-control-session <original-mxf> <proxy-mp4> \
  --script load,prepare,cue:0,preroll,play:5,pause,seek:50,preroll,play:5,stop,unload
```

Default script:

```text
load,prepare,cue:0,preroll,play:10,pause,seek:50,preroll,play:10,stop,unload
```

## Supported Script Commands

- `load`
- `prepare`
- `cue:<frame>`
- `preroll`
- `play:<frames>`
- `pause`
- `seek:<frame>`
- `stop`
- `unload`
- `snapshot`
- `tick:<frames>`
- `status`

Rejected commands print `accepted=no`, the reason/fault, whether the public
runtime state mutated, and then the script continues unless a fatal error is
introduced by future runtime work.

## Snapshot Output

Each command prints a compact runtime snapshot:

- accepted yes/no
- status
- source loaded yes/no
- current frame
- original-audio sample range
- prepared window
- video/audio payload readiness
- buffer state for tick/play rows
- rejection reason if rejected

Private local filesystem paths are never printed.

## Sample Compact Output

```text
QGS Broadcast Player Control Session
mode: ProxyPreview
video: proxy MP4
audio: original MXF mono lanes
device: preview-qnc-os
display: NotImplemented / target Wayland+Vulkan
private source path: hidden

SCRIPT
load,prepare,cue:0,preroll,play:10,pause,seek:50,preroll,play:10,stop,unload

[001] load
accepted=yes status=Loaded source=yes frame=<none> audio=none window=<none> video_ready=no audio_ready=no

[002] prepare
accepted=yes status=Loaded phase=input-ready source=yes

[003] cue:0
accepted=yes status=Ready source=yes frame=0 audio=[0..960) window=<none> video_ready=no audio_ready=no

[004] preroll
accepted=yes status=Ready window=[0..6) video_ready=yes audio_ready=yes

[005] play:10
accepted=yes status=Playing
  000 frame=0 audio=[0..960) window=[0..6) buffer=ready

Final:
commands accepted: 11
commands rejected: 0
run result: completed
private path exposed: no
real display: NotImplemented
visual verified: no
realtime verified: no
audio production verified: no
A/V sync verified: no
FramePresented real display claim: no
```

## Media Policy

The control session preserves the current QGS media truth:

- `ProxyPreview` uses proxy MP4 video for responsive preview picture.
- Original MXF audio is authoritative.
- Original MXF mono lanes remain discrete and individually addressable.
- Proxy MP4 AAC is non-authoritative.
- Original media paths remain private and are represented by public source
  identity only.

## Device And Display Truth

The command preserves the current non-claims:

- real display remains `NotImplemented`
- QNC OS display target remains Wayland + Vulkan
- X11 remains legacy/non-target
- visual verified: no
- realtime verified: no
- production audio verified: no
- A/V sync verified: no
- real backend `FramePresented`: no

## Difference From Fixed Run Mode

Fixed run mode is an operator-facing demonstration of one scenario. Control
session mode is a command script that acts more like a future QNC controller:
commands are explicit, snapshots are printed after each command, and rejected
commands remain visible.

Both modes use the same operational runtime.

## Next Recommended Step

The next step should keep shaping the Broadcast Player control surface around
real operator workflows while preserving the backend/device truth boundaries.
IPC, QNC UI, Wayland/Vulkan, realtime certification, A/V sync certification,
and production audio output should remain separate milestones.

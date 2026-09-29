# M2 Integration Block K — Broadcast Player Running Runtime Demo

Block K adds a runnable QGS Broadcast Player Runtime demo command:

```text
cargo run -q -p qgs-test -- --qgs-broadcast-player-run <original-mxf> <proxy-mp4>
```

The command is intentionally shaped like a backend player run, not a static
inspection report. It drives the existing `QgsBroadcastPlayerOperationalRuntime`
through a deterministic scenario and prints live runtime state after each step.

This is still not realtime playback, real display output, production audio
output, A/V sync, or QNC UI integration.

## Relationship To Prior Blocks

Block K builds directly on:

- Phase 22 input planning and source roles.
- Phases 23-28 command/session skeletons.
- Integration Block G control core.
- Integration Block H device backend selection.
- Integration Block I operational runtime.
- Integration Block J fault and recovery rules.

It does not introduce a second player model. The demo uses the operational
runtime methods and snapshots already established by those blocks.

## Command Scenario

The command runs this fixed scenario:

1. `LoadPreparedInput`
2. `Prepare`
3. `Cue` frame 0
4. `Tick` preparation until ready
5. `Play`
6. Run 25 logical playing ticks by default
7. `Pause`
8. Print two paused ticks and verify position does not advance
9. `Seek` to a later safe frame
10. Prepare/tick after seek as needed
11. `Play` again
12. Run 25 more logical playing ticks by default
13. `Stop`
14. `Unload`

The default seek frame for Sony FX6 sample 002 is frame 50. The command keeps
the run bounded and does not process the full file.

## Runtime Output

Each step reports:

- accepted/rejected command result
- operational status
- source loaded state
- current frame
- current original-audio sample range
- prepared frame window
- video/audio payload readiness
- buffer readiness and underrun status
- backend warning summary

The output also repeats the core media truth:

- mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF discrete mono lanes
- original MXF audio is authoritative
- proxy AAC is not authoritative
- private local paths are hidden from public output

## K2 Output Polish

Block K2 keeps the same runtime behavior and improves the command's readability.
No backend feature, presenter, audio output policy, realtime scheduler, or A/V
sync evidence was added.

The command now separates the early stages explicitly:

- `[PREPARE INPUT]` means the input was validated and the plan is available,
  but the runtime is still waiting for cue/preroll readiness. The internal
  status can honestly remain `Loaded` here.
- `[CUE]` records the selected frame and original-audio sample range. If the
  internal state reports `Ready`, the output also labels the phase as `Cued` so
  the transition is easier to read.
- `[PREROLL / PREPARE WINDOW]` is the bounded prepared-window step where video
  and audio payload readiness become visible.
- `[PREROLL / PREPARE WINDOW AFTER SEEK]` refreshes the prepared window around
  the seek target before playback resumes.

Persistent backend/device warnings are printed once near the header:

- real display unavailable until a Wayland/Vulkan backend exists
- production audio output not verified
- visual verification unavailable
- realtime verification unavailable
- A/V sync not verified
- selected real backend not implemented

Per-tick output now prints `warnings: unchanged` unless a new non-persistent
warning appears. This avoids making the player look like it is fault-spamming
while preserving the same conservative warning facts.

The final summary still reports:

- private path exposed: no
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio production verified: no
- A/V sync verified: no
- real-display `FramePresented` claim: no
- run result: completed
- runtime behavior evidence: yes

## Operator Run Mode

Block L turns the same command into a more usable operator-facing runtime mode.
It still drives the existing `QgsBroadcastPlayerOperationalRuntime`; qgs-test
only parses options, orchestrates commands, and prints the run.

```text
cargo run -q -p qgs-test -- --qgs-broadcast-player-run <original-mxf> <proxy-mp4> --frames 10 --seek-frame 50 --view compact
```

Options:

- `--frames <count>` controls how many logical frames are run before pause and
  after seek. The default is 25.
- `--seek-frame <frame>` controls the seek target after pause. The default is
  frame 50 when valid; shorter clips pick a safe bounded frame.
- `--view compact|detailed` selects the output style. `compact` is the default
  operator mode. `detailed` preserves the richer K/K2 report with persistent
  warnings and per-command details.

Compact mode presents the backend player as a running status stream:

```text
QGS Broadcast Player Runtime
mode: ProxyPreview
video: proxy MP4
audio: original MXF mono lanes
device: preview-qnc-os
display: NotImplemented / target Wayland+Vulkan

LOAD     status=Loaded source=yes
PREPARE  status=Loaded phase=input-ready
CUE      frame=0 audio=[0..960)
PREROLL  ready=yes window=[0..6)

PLAY
000  frame=0    audio=[0..960)        window=[0..6)  buffer=ready
001  frame=1    audio=[960..1920)     window=[0..6)  buffer=ready

PAUSE    frame=10 audio=[9600..10560)
PAUSED   position_changed=no

SEEK     10 -> 50 audio=[48000..48960)
PREROLL  ready=yes window=[48..56)

PLAY
000  frame=50   audio=[48000..48960)  window=[48..56) buffer=ready

STOP     status=Stopped source=yes
UNLOAD   status=Empty source=no

Final:
run result: completed
private path exposed: no
real display: NotImplemented
visual verified: no
realtime verified: no
audio production verified: no
A/V sync verified: no
```

Operator mode does not change the runtime behavior or evidence level. It does
not add a backend, real display output, realtime certification, A/V sync
certification, production audio output, QNC UI integration, export/render, or a
real backend `FramePresented` claim.

## QNC Control Session

M2 Block L adds a related command:

```text
cargo run -q -p qgs-test -- --qgs-broadcast-player-control-session <original-mxf> <proxy-mp4>
```

Fixed run mode shows the backend player behavior through one operator scenario.
Control session mode shows QNC-style command control: qgs-test sends explicit
commands such as `load`, `prepare`, `cue:0`, `play:10`, `pause`, `seek:50`,
`stop`, and `unload`, then prints the resulting runtime snapshot after each
command. Both modes use the same operational runtime and preserve the same
truth boundaries.

## Sony FX6 Sample 002 Result

For sample 002, the run reaches:

- `Loaded`
- `Ready`
- `Playing`
- `Paused`
- `Ready` after seek/reprepare
- `Playing`
- `Stopped`
- `Empty` after unload

The first run advances logical frames from frame 0 through frame 25, with audio
sample ranges advancing by 960 samples per 50p source frame. While paused, tick
commands do not advance the current frame. After seeking to frame 50 and
repreparing, the prepared window covers the seek target, playback resumes, and
the second run advances to frame 75 before stop.

`Stop` keeps the source loaded. `Unload` clears the source.

## Mironik 2002 Result

The same command is intended to run against the Mironik 2002 original/proxy pair
as a longer-source sanity check. It should demonstrate the same bounded runtime
behavior without exposing private paths or upgrading any output claims.

## Non-Claims

Block K does not claim:

- real display output
- real backend `FramePresented`
- visual verification
- realtime playback
- full Broadcast Player playback
- A/V sync
- production PipeWire/audio output
- `AudioDeviceVerified`
- QNC UI integration
- export/render
- Wayland/Vulkan presenter implementation
- X11 support for QNC OS

QNC OS display direction remains Wayland + Vulkan. X11 remains legacy/non-target.

## Verification Matrix

Step 20Q adds:

- subsystem: `broadcast player running runtime demo`
- evidence level: `RuntimeBehaviorEvidence`

This proves deterministic running backend runtime behavior only. It remains
below `VisualVerified`, `AudioDeviceVerified`, and `RealtimeVerified`.

## Next Step

The next work should continue shaping the production Broadcast Player control
surface without losing the truth boundaries established here. Real device
backends should remain explicitly separated from runtime behavior evidence until
they have their own verification.

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

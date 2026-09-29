# M2 Integration Block G — QGS Broadcast Player Control Core

Block G adds a production-shaped Broadcast Player Control Core facade for QGS.
It is the first layer intended to feel like the backend control surface of the
future QGS/QNC OS Broadcast Player instead of another isolated diagnostic
probe.

This block does not add realtime playback, real display output, speaker output,
Wayland/Vulkan presentation, QNC UI integration, export/render, or A/V sync. It
coordinates existing runtime Lego modules and exposes command, snapshot,
readiness, position, prepared-window, device-status, and event facts in a
QNC-safe shape.

## Relationship to the Lego modules

`QgsBroadcastPlayerCore` is a facade over the already separated modules:

- prepared input and input plan validation
- transport engine state and cue/seek readiness
- command execution and generation-checked session runtime
- bounded preparation/tick window
- event projection and passive view
- presenter, audio, and payload boundary status facts
- visual and audio diagnostics as evidence below real output verification

The core does not own decode internals, original/proxy mapping rules, frame
clock math, presenter backend internals, audio-device internals, QNC UI
concepts, or database/workflow concepts.

## Media policy

The control core preserves the current QGS media policy:

- `ProxyPreview` uses proxy MP4 video for responsive preview/edit picture.
- Original MXF audio is authoritative in all modes.
- `OriginalMedia` uses original MXF video plus original MXF audio for future
  finishing/original-quality workflows.
- Proxy MP4 AAC remains diagnostic/fallback only and is not authoritative.
- Broadcast/news audio remains discrete mono-channel based. Original MXF tracks
  remain individually addressable mono lanes; no stereo collapse or desktop
  L/R helper becomes runtime truth.

## Product commands

The public control command enum is `QgsBroadcastPlayerCommand`:

- `LoadPreparedInput`
- `Prepare`
- `Cue`
- `Play`
- `Pause`
- `Seek`
- `Stop`
- `Unload`
- `Tick`

Each product command maps to existing lower-level Phase 22-28 session and
transport commands. `Play` still requires readiness, and `Tick` prepares a
bounded window; it does not imply realtime scheduling.

## Status model

`QgsBroadcastPlayerStatus` exposes:

- `Empty`
- `Loaded`
- `Preparing`
- `Ready`
- `Playing`
- `Paused`
- `Stopped`
- `Completed`
- `Failed`

The current skeleton reaches the states needed by the deterministic control
scenario. Completion is reserved for future end-of-range or full runtime
completion policy.

## Snapshot fields

`QgsBroadcastPlayerSnapshot` reports:

- player status
- public source URI only
- source mode
- selected playback representation
- active range
- cue frame
- current frame
- current audio sample range
- rational frame rate
- audio sample rate
- readiness summary
- prepared window summary
- device status summary
- event counters
- lower-level passive view
- private-path exposure flag

Private filesystem paths must not appear in public command, event, or snapshot
output.

## Readiness fields

`QgsBroadcastPlayerReadiness` reports:

- input valid
- source loaded
- transport ready
- cue ready
- prepared window ready
- video payload ready
- audio payload ready
- presenter backend ready
- audio backend ready
- real display ready
- audio device verified
- visual verified
- realtime verified
- A/V sync verified
- proxy AAC authoritative
- discrete mono audio

The readiness model is conservative. Test and diagnostic evidence does not
become real display output, production audio-device verification, realtime
playback, or A/V sync.

## Device status fields

`QgsBroadcastPlayerDeviceStatus` reports:

- test presenter boundary available
- file presenter diagnostic available
- GPU readback diagnostic available
- real display backend: `NotImplemented`
- QNC OS display target: `Wayland + Vulkan`
- X11 target: `no / legacy non-target`
- visual verified: `false`
- realtime verified: `false`
- audio device production verified: `false`
- A/V sync verified: `false`

Block G deliberately does not proceed to Wayland/Vulkan, X11, DRM/KMS, or a
realtime presenter. X11 remains legacy/non-target for QNC OS.

## Product events

The control core emits product-level events:

- `PlayerLoaded`
- `PlayerPreparing`
- `PlayerReady`
- `PlayerCued`
- `PlayerStarted`
- `PlayerPaused`
- `PlayerSeeked`
- `PlayerStopped`
- `PlayerUnloaded`
- `PlayerFailed`
- `PlayerTicked`

These events wrap lower-level runtime/session facts. They do not claim
`FramePresented`, audible playback, real display output, or realtime
scheduling.

## qgs-test command

The control-core report is available through:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-control-core <original-mxf> <proxy-mp4>
```

The command builds the Phase 22 prepared descriptor and `InputPlan`, creates a
Broadcast Player Control Core session, and runs this deterministic sequence:

1. `LoadPreparedInput`
2. `Prepare`
3. `Cue frame 0`
4. `Tick`
5. `Play`
6. `Tick`
7. `Pause`
8. `Seek`
9. `Prepare`
10. `Play`
11. `Stop`
12. `Unload`

The report prints command acceptance, product events, final passive snapshot,
readiness, prepared-window state, device status, and explicit non-claims.

## Sony FX6 sample 002 acceptance result

Observed result:

- player surface: production-shaped backend control facade
- video source mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF
- proxy AAC: not used / not authoritative
- broadcast audio model: discrete original mono lanes
- command count: 12
- accepted commands: 12
- rejected commands: 0
- product events: 13
- final status: `Empty` after `Unload`
- source mode: `ProxyPreview`
- selected representation: `Proxy`
- frame rate: `50/1`
- audio sample rate: `48000`
- private path exposed: no
- real display: `NotImplemented`
- QNC OS display target: `Wayland + Vulkan`
- X11 target: `no / legacy non-target`
- visual verified: no
- realtime verified: no
- audio device production verified: no
- A/V sync verified: no
- device output: no
- real-display `FramePresented`: no

## Mironik 2002 acceptance result

Observed result:

- player surface: production-shaped backend control facade
- video source mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF
- proxy AAC: not used / not authoritative
- broadcast audio model: discrete original mono lanes
- command count: 12
- accepted commands: 12
- rejected commands: 0
- product events: 13
- final status: `Empty` after `Unload`
- source mode: `ProxyPreview`
- selected representation: `Proxy`
- frame rate: `50/1`
- audio sample rate: `48000`
- private path exposed: no
- real display: `NotImplemented`
- QNC OS display target: `Wayland + Vulkan`
- X11 target: `no / legacy non-target`
- visual verified: no
- realtime verified: no
- audio device production verified: no
- A/V sync verified: no
- device output: no
- real-display `FramePresented`: no

## Verification matrix update

Step 20Q adds:

- subsystem: `broadcast player control core`
- evidence level: `ControlSurfaceEvidence`

This means the control facade is command/snapshot/event verified, not real
display, realtime, A/V sync, or production audio-device verified.

## Recommended next block

The next useful block should build on this control core rather than adding
another standalone diagnostic. A sensible direction is a bounded control-core
readiness adapter that wires prepared payload/device-boundary evidence into the
core snapshot without changing the conservative real-output truth rules.

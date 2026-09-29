# M2 Integration Block I — QGS Broadcast Player Operational Runtime

Block I adds a deterministic operational runtime layer over the Broadcast
Player Control Core and device-backend selection model. It turns the
production-shaped control surface from Block G into executable backend runtime
behavior: command legality, logical tick/position progression, seek/stop/unload
semantics, buffer-health reporting, and conservative warnings.

This is not realtime playback, real display output, speaker output, A/V sync,
Wayland/Vulkan presentation, QNC UI integration, export/render, or a new
diagnostic path.

## Relationship to Earlier Blocks

The operational runtime builds on the existing runtime Lego modules:

- Phase 22 prepared input and `InputPlan`
- Phase 23-28 source, transport, range, cue, command, and event surfaces
- Integration Block G Broadcast Player Control Core
- Integration Block H device backend selection

It does not invent a second player/input model. Commands are applied through the
existing Broadcast Player Control Core and lower-level session/transport
surfaces.

## Media Policy

The runtime preserves the current QGS media policy:

- `ProxyPreview` uses proxy MP4 video for responsive preview/edit picture.
- Original MXF audio is authoritative.
- `OriginalMedia` remains the original MXF video plus original MXF audio mode
  for future finishing/original-quality workflows.
- Proxy MP4 AAC is diagnostic/fallback only and is not authoritative.
- Broadcast/news audio remains discrete mono-channel based. Original MXF tracks
  remain individually addressable mono lanes; no stereo collapse or desktop
  helper becomes runtime truth.

## Operational Status Model

The runtime reports the Broadcast Player status from the control core and adds
operational accounting around it:

- command acceptance and rejection counts
- current logical frame
- current original-audio sample range
- completion flag
- bounded prepared-window health
- backend/runtime warnings
- private-path exposure flag

Invalid commands do not mutate runtime state. `Play` before a ready source is
rejected. `Tick` only advances logical frame/audio position while the player is
`Playing`.

## Command Legality

The operational runtime enforces these core rules:

| Command | Operational Rule |
| --- | --- |
| `LoadPreparedInput` | accepted from `Empty`/unloaded states |
| `Prepare` | requires a loaded source and builds the ready/prepared state |
| `Cue` / `Seek` | require a loaded source and must target the active range |
| `Play` | requires `Ready` or `Paused`; rejected from `Empty`/`Loaded` |
| `Pause` | accepted only from `Playing` |
| `Tick` | accepted in deterministic scenarios but advances only while `Playing` |
| `Stop` | stops transport and keeps the current source loaded |
| `Unload` | clears the active source and returns to `Empty` |

Rejected commands produce a reason and leave the snapshot unchanged except for
rejection accounting.

## Tick and Position Model

Each accepted playing tick advances the logical frame by the configured tick
step. For the Sony FX6 sample 002 `ProxyPreview` path:

- proxy frame rate: `50/1`
- audio sample rate: `48000`
- one proxy frame maps to 960 original-audio samples

The current audio range is derived from the current frame timestamp and the
authoritative original MXF audio sample rate. This is logical runtime
accounting, not realtime scheduling or A/V sync certification.

## Prepared Window and Buffer Health

The runtime reports bounded prepared-window health:

- prepared start frame
- prepared end frame
- selected prepared frame
- prepared frame count
- low-water state
- underrun state
- discarded-frame count
- preparation-pending state

This is a bounded operational health model over prepared runtime records. It is
not a real decoded video queue, production audio ring buffer, or realtime
scheduler.

## Seek, Stop, and Completion

Seek updates the logical position only when the target lies inside the active
range. A seek outside the active range is rejected.

Stop freezes the current logical position and keeps the source loaded. Unload
clears the source and returns the runtime to `Empty`.

Completion is modeled when ticks reach the active range end. Completion keeps
real display, realtime, A/V sync, and production audio-device claims false.

## Warning and Fault Model

The runtime reports conservative warnings when production backends are not
available:

- real display backend not implemented
- real display unavailable
- production audio output not verified
- prepared-window underrun when applicable
- illegal command reasons
- source/range/cue failures

These warnings are public-safe and must not include raw private filesystem
paths.

## Snapshot Surface

The operational snapshot includes:

- player status
- source mode
- public source URI only
- selected representation
- active range
- current frame and original-audio sample range
- readiness facts
- prepared-window/buffer health
- device backend selection
- operational faults/warnings
- command acceptance/rejection counts
- completion flag
- private-path exposure flag

QNC applications may later observe this surface and issue commands. They do not
own media readiness, original/proxy mapping, backend timing facts, prepared
runtime state, or device/clock policy.

## qgs-test Command

The operational runtime report is available through:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-operational-runtime <original-mxf> <proxy-mp4>
```

The command runs this deterministic scenario:

1. illegal `Play` while `Empty`
2. `LoadPreparedInput`
3. `Prepare` with `[0..1000 ms)` active range
4. `Cue frame 0`
5. `Tick`
6. `Play`
7. several playing ticks
8. `Pause`
9. paused tick, which does not advance
10. `Seek` later inside the active range
11. `Prepare`
12. `Play`
13. bounded playing ticks
14. `Stop`
15. `Unload`

The report prints command counts, acceptance/rejection reasons, status
transitions, frame/audio progression, buffer health, warnings, source-loaded
policy after `Stop`, source-unloaded policy after `Unload`, and all production
non-claims.

## Sony FX6 Sample 002 Result

Observed result for the Sony FX6 sample 002:

- video source mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF
- original MXF audio authoritative: yes
- proxy AAC authoritative: no
- command count: 20
- accepted commands: 19
- rejected commands: 1
- rejected command: illegal `Play` while `Empty`
- status sequence: `Empty -> Loaded -> Loaded -> Ready -> Ready -> Playing -> Playing -> Playing -> Playing -> Paused -> Paused -> Paused -> Paused -> Ready -> Playing -> Playing -> Playing -> Playing -> Stopped -> Empty`
- logical position advanced only while `Playing`
- one 50 fps proxy frame mapped to 960 original-audio samples
- stopped source remained loaded: yes
- unloaded source cleared: yes
- private path exposed: no
- real display: `NotImplemented`
- QNC OS display target: `Wayland + Vulkan`
- X11 target: no / legacy non-target
- visual verified: no
- realtime verified: no
- audio device production verified: no
- A/V sync verified: no
- device output: no
- real-display `FramePresented` claim: no

## Mironik 2002 Result

The same operational runtime scenario was run against Mironik 2002. The result
matched the sample 002 operational shape:

- command count: 20
- accepted commands: 19
- rejected commands: 1
- rejected command: illegal `Play` while `Empty`
- status sequence matched the sample 002 sequence
- low-water state: no
- underrun state: no
- stopped source remained loaded: yes
- unloaded source cleared: yes
- private path exposed: no
- real display, realtime playback, production audio-device verification, and
  A/V sync remained false

## Verification Matrix

Step 20Q adds:

- subsystem: `broadcast player operational runtime`
- evidence level: `OperationalStateEvidence`

This means deterministic command legality, logical tick/position progression,
seek/stop/completion state, buffer-health reporting, and warnings are modeled
and tested. It does not mean realtime playback, real display output,
production audio output, visual verification, or A/V sync.

## Non-Claims

Block I does not claim:

- Wayland/Vulkan presenter implementation
- X11 presenter implementation
- DRM/KMS presenter implementation
- real display output
- real backend `FramePresented`
- visual verification
- realtime playback
- A/V sync
- speaker output
- production PipeWire audio output
- `AudioDeviceVerified`
- QNC UI integration
- export/render

## Next Recommended Block

M2 Integration Block J — Broadcast Player Fault and Recovery Rules.

The next work should formalize fault categories, recoverable vs fatal errors,
source/backend recovery behavior, retry policy, and public-safe recovery events
before any real Wayland/Vulkan presenter work.

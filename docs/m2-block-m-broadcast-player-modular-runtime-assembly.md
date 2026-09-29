# M2 Block M - Broadcast Player Modular Runtime Assembly

Block M introduces `QgsBroadcastPlayerAssembly`, an explicit composition root
for the QGS Broadcast Player Runtime.

This block moves QGS toward a real Broadcast Player product shape without
turning the runtime into one monolithic player object. The assembly wires
existing LEGO modules together; it does not absorb their behavior.

## Why This Exists

Recent M2 blocks proved many working pieces: input planning, transport parity,
frame/audio timing, prepared windows, operational runtime behavior, fault and
recovery rules, device-backend selection, operator run mode, and QNC-style
control sessions.

Those pieces were correct, but the product concept was spread across many
surfaces. Block M restores the "broadcast engine" as a composition concept:
not a monolithic implementation, but a clear place where the QGS Broadcast
Player Runtime is assembled from replaceable backend modules.

## Assembly Vs Monolith

`QgsBroadcastPlayerAssembly` is a composition root.

It may:

- hold an `InputPlan`
- hold runtime/device configuration
- construct the existing operational runtime
- expose module inventory and public-safe assembly facts
- expose device-selection facts

It must not:

- implement frame-clock math
- implement transport state-machine internals
- implement preroll/window internals
- decode media
- map audio channels itself
- implement presenter or audio-device internals
- duplicate QNC control-session logic
- expose private local paths

## Assembly Vs Operational Runtime

The operational runtime performs player behavior: command legality, logical
position, prepared-window health, faults, stop/unload behavior, and snapshots.

The assembly creates that runtime with the selected input plan and configuration.
It is not a second runtime and does not own command execution semantics.

## Assembly Vs qgs-test

`qgs-test` remains a CLI orchestrator and printer. It still performs acceptance
media binding from local test files, then hands a public-safe descriptor/plan to
the assembly.

`qgs-test` should not become the player. It should use the assembly where
practical, issue commands, and print snapshots.

## Assembly Vs QNC

QNC applications may later issue session/transport commands, observe events and
snapshots, and present UI state. QNC does not own QGS backend timing facts,
prepared state, source-mode validation, or device policy.

The assembly is the backend composition layer that future QNC-facing control
surfaces can use without importing QNC crates into QGS.

## LEGO Modules

| Module | Owns | Does Not Own |
| --- | --- | --- |
| `input_plan` | Public source identity, source mode, original/proxy timing facts. | Transport behavior or device output. |
| `transport` | Load, prepare, cue, play, pause, seek, stop, unload. | Decode, device output, or work-on-Play. |
| `frame_clock` | Logical frame progression and frame/sample mapping facts. | Realtime certification. |
| `preroll` | Bounded prepared window and no-play-before-ready facts. | Media decode or device queues. |
| `video_payload` | ProxyPreview payload status now, OriginalMedia later. | Real display presentation. |
| `audio_payload` | Original MXF mono-lane payload facts. | Proxy AAC runtime truth. |
| `audio_device_boundary` | PipeWire prototype facts and swappable audio boundary. | Production `AudioDeviceVerified`. |
| `presenter_boundary` | Test/file/readback diagnostics and future presenter edge. | Real display output. |
| `device_selection` | Preview, diagnostic, headless, and future backend policy. | Backend implementation. |
| `fault_recovery` | Structured faults and recovery suggestions. | Silent recovery. |
| `event_surface` | QNC-shaped public-safe events. | Private path output. |
| `snapshot_projection` | Operator/QNC-readable passive state. | Runtime mutation. |
| `session_facade` | Clean control surface for qgs-test and future QNC apps. | Module internals. |

## Allowed Dependencies

- The assembly may depend on backend-neutral runtime types.
- The assembly may construct `QgsBroadcastPlayerOperationalRuntime`.
- The assembly may select backend policy through `QgsDeviceBackendSelector`.
- qgs-test may build an assembly from acceptance file paths via the existing
  prepared input descriptor builder.

## Forbidden Dependencies

- No QNC crate imports.
- No PipeWire/Wayland/X11/DRM/KMS dependencies in `qgs-media-runtime`.
- No media probing inside the assembly.
- No proxy AAC as authoritative runtime audio.
- No stereo desktop helper as broadcast audio truth.
- No real display, realtime, A/V sync, or production audio claims.

## Operator Run Usage

`--qgs-broadcast-player-run` now builds a `QgsBroadcastPlayerAssembly` and asks
it for an operational runtime. The command still drives the same deterministic
operator scenario and prints runtime state.

The output includes a short assembly line and module inventory. Runtime behavior
is unchanged.

## QNC Control Session Usage

`--qgs-broadcast-player-control-session` also builds a
`QgsBroadcastPlayerAssembly` and asks it for an operational runtime. The script
parser and snapshot printing remain qgs-test responsibilities.

## Media Policy

- `ProxyPreview` uses proxy MP4 video for responsive preview picture.
- Original MXF audio is authoritative.
- Original MXF mono lanes remain discrete and individually addressable.
- Proxy AAC is non-authoritative.
- Private local paths remain hidden behind public QNC-style source identity.

## Device And Display Policy

- QNC OS display target remains Wayland + Vulkan.
- X11 remains legacy/non-target.
- Real display output remains `NotImplemented`.
- Visual verification remains no.
- Realtime verification remains no.
- Production audio verification remains no.
- A/V sync verification remains no.

## Non-Claims

Block M does not implement:

- Wayland/Vulkan presenter
- X11 presenter
- DRM/KMS presenter
- realtime scheduler
- A/V sync certification
- production audio output
- QNC UI integration
- export/render
- new media decode paths

## Acceptance Result

For Sony FX6 sample 002 and Mironik 2002, the assembly-backed operator/control
commands preserve previous behavior:

- operator run still completes
- QNC control session still completes
- private path exposed: no
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio production verified: no
- A/V sync verified: no

## Next Recommended Step

The next step should keep replacing diagnostic edges with production-shaped
modules one boundary at a time. The real player emerges by swapping LEGO blocks,
not by merging the runtime into a monolith.

# M2 Integration Block H — Broadcast Player Device Backend Selection

Block H adds a backend-neutral device backend selection model for the QGS
Broadcast Player Runtime. Its purpose is to stop ad-hoc device probing from
becoming product policy. The Broadcast Player Control Core can now report a
stable device-selection snapshot without owning real device backend internals.

This block does not implement Wayland/Vulkan, X11, DRM/KMS, realtime playback,
A/V sync, QNC UI, export/render, or production audio output.

## Why This Exists

Earlier blocks proved useful boundaries:

- test presenter evidence
- file presenter diagnostics
- GPU readback diagnostics
- native PipeWire prototypes and bounded audio evidence
- Broadcast Player Control Core snapshots

Those are not the same as production backend selection. Block H separates
diagnostic availability from future production targets and gives QGS a single
truthful policy surface for device backend choice.

## Ownership Boundary

QGS owns backend media/runtime readiness, prepared payload readiness, device
boundary evidence, and future device/clock policy. QNC applications may later
issue commands, observe snapshots/events, and present UI state.

QNC UI does not select private device implementation details, does not own
media timing facts, and does not turn diagnostic evidence into production
verification.

## Backend Candidate Model

Video presenter candidates:

- `TestPresenter`
- `FilePresenterDiagnostic`
- `GpuReadbackDiagnostic`
- `WaylandVulkanPresenter`
- `DrmKmsVulkanPresenter`
- `X11LegacyNonTarget`

Audio output candidates:

- `NoAudioOutput`
- `TestAudioSink`
- `PipeWireCommandPrototype`
- `PipeWireNativePrototype`
- `PipeWireProductionFuture`

Candidate availability is represented with:

- `Available`
- `AvailableDiagnosticOnly`
- `NotImplemented`
- `UnsupportedForQncOs`
- `MissingRuntimeDependency`
- `HardwareUnavailable`
- `NotProductionVerified`
- `DisabledByPolicy`

## Selection Policies

`DiagnosticOnly`

- selected video: `GpuReadbackDiagnostic`
- selected audio: `TestAudioSink`
- fallback: file presenter diagnostic / no audio output
- real display ready: no
- production audio ready: no

`PreviewOnQncOs`

- selected video: `WaylandVulkanPresenter`
- selected video availability: `NotImplemented`
- selected audio: `PipeWireProductionFuture`
- selected audio availability: `NotImplemented`
- diagnostic fallback: `GpuReadbackDiagnostic` / `TestAudioSink`
- X11 is not selected

`OriginalMediaOnQncOs`

- selected video: `WaylandVulkanPresenter`
- selected video availability: `NotImplemented`
- selected audio: `PipeWireProductionFuture`
- selected audio availability: `NotImplemented`
- preserves OriginalMedia as a valid QGS runtime mode without claiming realtime
  original-video presentation

`HeadlessCi`

- selected video: `TestPresenter`
- selected audio: `NoAudioOutput`
- deterministic and non-real-display

`FutureApplianceDirect`

- selected video: `DrmKmsVulkanPresenter`
- selected video availability: `NotImplemented`
- selected audio: `PipeWireProductionFuture`
- selected audio availability: `NotImplemented`

## Display Policy

QNC OS display target is Wayland + Vulkan.

X11 is legacy/non-target only and is never selected by the QNC OS policies.
DRM/KMS + Vulkan remains a possible future direct/appliance path.

This block keeps:

- real display output: no
- real backend `FramePresented`: no
- `VisualVerified`: no
- `RealtimeVerified`: no

## Audio Policy

Original MXF audio remains authoritative. Proxy MP4 AAC is diagnostic/fallback
only and is never authoritative.

PipeWire command/native prototypes remain `NotProductionVerified` or future
`NotImplemented` candidates. They do not upgrade production audio readiness or
`AudioDeviceVerified`.

Broadcast/news audio remains discrete mono-channel based. Stereo desktop helper
paths are not runtime truth or production routing.

## Control Core Integration

`QgsBroadcastPlayerDeviceStatus` now includes a `QgsDeviceBackendSelection`.
The Broadcast Player Control Core currently uses `PreviewOnQncOs` because the
accepted runtime path is `ProxyPreview` with proxy MP4 video and authoritative
original MXF audio.

The snapshot reports:

- selection policy
- selected video backend
- selected audio backend
- diagnostic fallbacks
- unavailable reasons
- candidate availability
- QNC OS display target
- X11 target
- real display readiness
- real audio backend readiness
- visual/realtime/audio-production/A-V sync non-claims

No private local paths are exposed.

## qgs-test Command

Device selection can be inspected without media:

```bash
cargo run -q -p qgs-test -- --qgs-device-backend-selection diagnostic-only
cargo run -q -p qgs-test -- --qgs-device-backend-selection preview-qnc-os
cargo run -q -p qgs-test -- --qgs-device-backend-selection original-qnc-os
cargo run -q -p qgs-test -- --qgs-device-backend-selection headless-ci
cargo run -q -p qgs-test -- --qgs-device-backend-selection future-appliance-direct
```

The Broadcast Player Control Core report also prints the new device-selection
snapshot:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-control-core <original-mxf> <proxy-mp4>
```

## Current Results

Observed policy selections:

| Policy | Selected Video | Selected Audio | Real Display | Production Audio |
| --- | --- | --- | --- | --- |
| `diagnostic-only` | `GpuReadbackDiagnostic` / `AvailableDiagnosticOnly` | `TestAudioSink` / `AvailableDiagnosticOnly` | no | no |
| `preview-qnc-os` | `WaylandVulkanPresenter` / `NotImplemented` | `PipeWireProductionFuture` / `NotImplemented` | no | no |
| `original-qnc-os` | `WaylandVulkanPresenter` / `NotImplemented` | `PipeWireProductionFuture` / `NotImplemented` | no | no |
| `headless-ci` | `TestPresenter` / `AvailableDiagnosticOnly` | `NoAudioOutput` / `AvailableDiagnosticOnly` | no | no |
| `future-appliance-direct` | `DrmKmsVulkanPresenter` / `NotImplemented` | `PipeWireProductionFuture` / `NotImplemented` | no | no |

The Sony FX6 sample 002 and Mironik 2002 Broadcast Player Control Core reports
use `PreviewOnQncOs`, preserve original MXF audio as authoritative, do not use
proxy AAC as authoritative audio, expose no private paths, and keep real
display/audio/realtime/A-V sync claims false.

## Verification Matrix

Step 20Q adds:

- subsystem: `device backend selection`
- evidence level: `SelectionPolicyEvidence`

This means policy selection and conservative availability reporting are
implemented and tested. It does not mean any real device backend is implemented.

## Non-Claims

Block H does not claim:

- Wayland/Vulkan presenter implementation
- X11 implementation
- DRM/KMS implementation
- real display output
- real backend `FramePresented`
- visual verification
- realtime playback
- A/V sync
- production PipeWire audio output
- `AudioDeviceVerified`
- channel certification

## Next Recommended Block

M2 Integration Block I — Wayland + Vulkan Presenter Boundary Prototype.

That should happen only after this selection model exists. Wayland/Vulkan is the
first real QNC OS display target. X11 must not be implemented as a QNC OS
backend.

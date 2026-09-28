# M2 Step 20R — QGS Broadcast Player Runtime Stabilization Audit

Step 20R is a stabilization and naming audit for the Broadcast Player Runtime
work from Steps 20D through 20Q.

This milestone adds no new media feature. It clarifies command naming,
terminology, evidence levels, and readiness boundaries before real audio or
display device backend work begins.

## Scope

The audit covered:

- runtime type and status terminology
- source-mode naming
- test-boundary evidence wording
- qgs-test command naming
- Step 20 documentation titles and command examples
- verification matrix truthfulness
- future code organization risks

No realtime scheduler, real audio output, display output, export/render, QNC UI
integration, QNC crate dependency, new media decode path, or proxy-AAC primary
path was added.

## Changes Made

- Added preferred qgs-test command aliases for early Broadcast Player Runtime
  commands:
  - `--broadcast-player-runtime-contract`
  - `--broadcast-player-runtime-state-machine`
  - `--broadcast-player-runtime-preroll`
  - `--broadcast-player-runtime-prepared-slots`
- Kept the older `--broadcast-runtime-*` command names as compatibility aliases.
- Updated Step 20D and Step 20E command examples to use the preferred
  `broadcast-player-runtime` naming.
- Added this stabilization report.

No runtime behavior was intentionally changed.

## Terminology Decisions

Use consistently:

- QGS Broadcast Player Runtime
- `ProxyPreview`
- `OriginalMedia`
- `RuntimeAccountingReady`
- `PayloadReady`
- `DevicePayloadReady`
- `SubmittedToDevice`
- `PresentationEvidenceReceived`
- `FramePresented`
- `CapabilityMissing`
- `NotReady`
- `TestBoundaryEvidence`
- original MXF audio
- proxy MP4 video
- original MXF video

Avoid as primary terminology:

- generic "Broadcast Runtime" when the Broadcast Player Runtime is meant
- "playout" unless discussing final air-chain behavior
- "presented" when a frame was only accounted
- "audio played" or "heard" for test audio sink evidence
- "realtime verified" without a timed acceptance with margin

## Source Modes

`ProxyPreview`:

- video source: proxy MP4
- audio source: original MXF
- purpose: responsive journalist/edit preview

`OriginalMedia`:

- video source: original MXF
- audio source: original MXF
- purpose: powerful hardware, finishing, and original-quality workflows

Proxy MP4 AAC remains diagnostic/fallback only and is never authoritative.

## Readiness And Evidence Model

The runtime keeps these concepts separate:

- `RuntimeAccountingReady`: timing/accounting can proceed, but real payloads may
  not all be bound.
- `PayloadReady`: runtime slots bind to concrete backend payload references.
- `DevicePayloadReady`: a configured device boundary can accept those payloads.
- `SubmittedToDevice`: a payload was submitted to a sink/presenter boundary.
- `PresentationEvidenceReceived`: a sink/presenter returned evidence.
- `FramePresented`: emitted only after video presenter evidence exists.
- `CapabilityMissing`: a supported contract mode lacks a required backend.
- `NotReady`: required preconditions are not met.

Test presenter evidence is not real display output. Test audio sink evidence is
not real speaker output. `FramePresented` from the current test presenter is
test evidence only.

## QGS-Test Command Inventory

Preferred Broadcast Player Runtime commands:

- `--broadcast-player-runtime-contract <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-state-machine <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-preroll <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-prepared-slots <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-events <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-payloads <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-video-payloads <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-device-boundary <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-test-presenter <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-test-audio-sink <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-simulate <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-original-video-payloads <original-mxf> <proxy-mp4>`
- `--broadcast-player-runtime-verification <original-mxf> <proxy-mp4>`

Compatibility aliases retained:

- `--broadcast-runtime-contract`
- `--broadcast-runtime-state-machine`
- `--broadcast-runtime-preroll`
- `--broadcast-runtime-prepared-slots`

Those older names should not be used in new documentation.

## Documentation Audit

The Step 20 documentation filenames and titles now use Broadcast Player Runtime
wording for runtime milestones. Step 20D and Step 20E command examples were
updated to the preferred CLI names.

The audit did not find stale `journalist-25p` terminology in the Step 20 docs.
Existing Step 18 documentation correctly uses `journalist-50i-preview` for the
50i-compatible preview profile.

## Verification Matrix Check

Step 20Q remains the canonical truthfulness matrix:

- test video presenter evidence: `TestBoundaryEvidence`, not real display
- test audio sink evidence: `TestBoundaryEvidence`, not real speaker output
- original video payload binding: `PayloadBound`, not realtime
- real speaker output: `NotImplemented`
- real display output: `NotImplemented`
- realtime playback: `NotImplemented`
- modern-hardware zero-copy: `NotImplemented`

This is the intended baseline before device backend work.

## Code Organization Notes

`qgs-media-runtime` now contains several logical groups:

- playback timing and bounded queues
- original-audio metadata, packets, and PCM blocks
- Broadcast Player Runtime contract/session types
- preroll and prepared slots
- payload binding
- device boundary and test evidence
- simulated runtime loop
- verification matrix

No large refactor was performed in Step 20R. A later cleanup could split these
into internal modules once the real device-boundary shape is clearer.

## Remaining Cleanup Risks

- Some internal variable names still use shorter `broadcast_runtime_*` wording
  where the context is local and not user-facing. This is acceptable for now,
  but a later module split can rename internals more broadly.
- The qgs-test parser is intentionally explicit and repetitive. A future CLI
  helper may reduce duplication, but Step 20R avoids that refactor.
- `FramePresented` remains a valid event name, but current usages must stay
  labeled as test-presenter evidence until a real presenter exists.

## Do Not Implement Next By Accident

Before the next approved milestone, do not add:

- real speaker output
- PipeWire, ALSA, or PulseAudio
- Vulkan swapchain, Wayland, X11, DRM/KMS, or display output
- realtime scheduler
- export/render
- QNC UI integration or QNC crate dependency
- proxy AAC as authoritative audio

## Recommended Next Milestone

The next milestone should choose one device boundary deliberately:

- real audio device boundary and audio-clock evidence, or
- real video presenter boundary and display evidence.

Either path should preserve the current terminology and verification matrix:
test evidence must not be upgraded to real-output evidence without an actual
device backend proving it.

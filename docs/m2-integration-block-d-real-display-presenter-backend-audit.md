# M2 Integration Block D - Real Display Presenter Backend Audit

Integration Block D audits real display presenter backend options for QGS. It is
an audit/planning block only.

It does not implement a real display backend, Wayland/X11/DRM/KMS output,
realtime playback, A/V sync, QNC UI, export/render, or a monolithic player path.
The presenter boundary remains its own module.

## Purpose

QGS is becoming the backend media/runtime/device foundation for the future
QGS/QNC OS Broadcast Player. Integration Blocks A, B, and C proved that session
control, prepared payload facts, presenter-boundary submission, and monitor
projection can be wired together without claiming real output.

The next risk is display ambiguity: a `TestPresenterAccepted` result can be
useful for module wiring, but it must not be mistaken for a real displayed
frame. This audit defines what a real presenter backend must own, compares
Linux display backend options, and recommends a safe implementation direction.

## Current QGS Presenter State

QGS currently has a backend-neutral presenter/monitor boundary:

- `QgsPresenterPayloadDescriptor`
- `QgsPresenterSubmission`
- `QgsPresenterSubmissionResult`
- `QgsPresenterEvidence`
- `QgsMonitorFrameDescriptor`
- `QgsMonitorProjectionUpdate`

The current concrete presenter is `TestPresenter`. It accepts a prepared video
payload descriptor and returns structured test evidence. That proves that the
presenter boundary was called with a valid payload record. It does not prove a
user-visible frame reached a display.

Current evidence remains:

- presenter/monitor boundary: `TestBoundaryEvidence`
- real display output: `NotImplemented`
- `VisualVerified`: not claimed
- `RealtimeVerified`: not claimed

Monitor projection is passive. It can expose prepared/submitted/test-evidence
facts, but it must not become the real display backend or the QNC UI.

## Current Payload Shape

The Block B/C presenter path receives a prepared video payload reference rather
than a real presenter-owned swapchain image.

Current runtime payload facts include:

- public source URI
- source frame index
- selected preview frame index, when applicable
- payload id
- payload kind: currently `ProcessedGpuFrame` for accepted proxy/original
  payload paths
- payload format: currently `RgbaU16`
- backend path:
  - `VaapiCpuNv12Vulkan` for `ProxyPreview`
  - `SoftwareH264Yuv422P10Vulkan` for bounded `OriginalMedia` payload binding
- presentation time and duration
- coded dimensions, commonly 1920x1088 for the Sony sample
- visible dimensions, commonly 1920x1080
- bounded slot/session identity

The Vulkan processing helpers currently produce RGBA-u16 output summaries and
readback-friendly records such as `Nv12GpuOutput`, `Yuv422P10GpuOutput`, and
`ProcessedFrameOutput`. These include `rgba_u16`, checksum, dimensions, byte
counts, frame token, slot index, and submission sequence.

What QGS does not yet expose as a display-ready object:

- swapchain image ownership
- a presenter-owned Vulkan image layout
- display queue ownership
- timeline semaphore/fence handoff to a present queue
- Wayland/X11/DRM/KMS surface identity
- presentation callback/evidence from a real compositor/display backend

## Missing Real Presenter Capabilities

A real display presenter needs more than the current payload descriptor:

- real window, surface, or headless display target creation
- swapchain or present target creation
- GPU image ownership or a copy/import path into the present target
- format negotiation from QGS `RgbaU16` or future GPU image format to display
  format
- color/transfer policy for monitor output
- synchronization between QGS processing work and presenter submission
- bounded frame lifetime and ownership handoff
- backpressure when the presenter cannot accept new frames
- resize and surface-loss handling
- display backend error reporting
- presentation callback or other evidence that a frame was accepted by a real
  presenter backend
- visual verification strategy separate from presentation callback evidence

`FramePresentedRealBackend` must require real presenter evidence. It must not be
inferred from payload readiness, GPU completion, monitor projection, or test
presenter acceptance.

## Backend Candidate Comparison

| Candidate | Strengths | Risks / Missing Work | Fit For QGS |
| --- | --- | --- | --- |
| Wayland + Vulkan WSI | Linux-first, strong fit for future QNC OS, avoids legacy X11 assumptions, natural with a Vulkan processing pipeline. | Requires surface/window lifecycle, compositor protocols, swapchain management, present callback/evidence semantics, resize handling, and careful separation from QNC UI. | Best first real display direction for QNC OS, once the presenter-boundary prototype is ready. |
| X11 + Vulkan WSI | Broad desktop compatibility and useful fallback on developer machines. | Less aligned with future QNC OS direction, extra legacy window-system concerns, weaker fit as the primary architecture target. | Useful fallback/diagnostic backend, not the first strategic backend. |
| DRM/KMS + Vulkan | Direct device path, strong appliance/broadcast-station potential, avoids desktop compositor. | Highest integration risk: permissions, mode setting, lease/session management, hotplug, VT ownership, and conflict with desktop UI. | Later dedicated output path, not the first journalist-laptop backend. |
| Vulkan swapchain abstraction directly | Keeps the presenter close to the GPU pipeline and exposes correct sync/present concepts. | Still needs a platform surface provider; by itself it does not solve Wayland/X11/DRM ownership. | Good internal layer below a Wayland-first backend. |
| `winit`/`wgpu` style window abstraction | Fast prototype path, easier window creation, cross-platform habits, possible screenshot hooks. | May hide WSI details QGS needs for evidence, timing, swapchain ownership, and QNC OS integration. `wgpu` may duplicate/abstract away existing Vulkan choices. | Useful spike candidate only if it stays isolated and does not become the production contract. |
| Headless/null presenter | CI friendly and deterministic. | Not real display output; cannot prove real presentation or visual correctness. | Keep as test boundary, not as real display milestone. |
| Screenshot/file presenter | Deterministic visual artifact, can compare pixels, works with current RGBA-u16 readback payloads, good for visual QA. | Not real display output; does not prove compositor/display submission or timing. | Best near-term diagnostic visual verification step before real swapchain work. |

## Recommended First Backend

Recommended real display direction: **Wayland + Vulkan presenter boundary**.

Reasons:

- QGS/QNC OS is Linux-first.
- Journalist laptops and future QNC OS desktops are expected to have a
  compositor/display session rather than exclusive KMS ownership.
- QGS already has a Vulkan processing path and processed frame tokens.
- Wayland keeps real display readiness in the QGS presenter boundary while QNC
  applications remain UI/workflow observers and command issuers.
- A Wayland presenter can later report real surface/swapchain/presentation
  evidence without importing QNC crates.

Recommended immediate next block: **M2 Integration Block E - Screenshot/File
Presenter Visual Diagnostic**.

That block should not claim real display output. It should use the current
RGBA-u16 processed payload/readback shape to produce a deterministic diagnostic
image or screenshot artifact and establish visual comparison rules. This creates
a low-risk visual baseline before the first Wayland/Vulkan surface prototype.

The first real display implementation after that should be a minimal
Wayland/Vulkan presenter prototype with one prepared frame, bounded lifetime,
no realtime loop, and explicit evidence levels up to `SurfaceCreated` or
`SwapchainReady` before attempting frame submission.

## Evidence Ladder

Future presenter work should use an explicit evidence ladder:

| Evidence | Meaning |
| --- | --- |
| `PresenterBackendUnavailable` | Required display backend is not available. |
| `PresenterBackendAvailable` | Backend libraries/session are reachable. |
| `SurfaceCreated` | A real presenter surface/window/output target exists. |
| `SwapchainReady` | A present target/swapchain is configured and ready. |
| `FrameSubmittedToPresenter` | A prepared frame was submitted to the real presenter backend. |
| `PresentationCallbackReceived` | The backend/compositor/display reported presentation or equivalent acceptance evidence. |
| `FramePresentedRealBackend` | QGS has real backend evidence for presentation of a frame. |
| `VisualVerified` | Image correctness was verified by screenshot, comparison, or other visual evidence. |
| `RealtimeVerified` | Timed realtime acceptance with margin was verified. |

None of these real display levels are claimed by this audit.

## QGS/QNC Ownership Boundary

QGS may own:

- presenter backend object
- surface/device readiness
- swapchain/present-target lifecycle
- frame submission
- frame lifetime and backpressure
- presenter evidence
- lower-level timing/accounting facts needed by future playback policy

QNC applications may later:

- issue session/transport commands
- observe runtime events
- present monitor/timeline UI state
- store/read media snapshots
- provide application-level workflow and layout decisions

QNC UI must not own media readiness, presenter readiness, display timing facts,
or QGS prepared payload state. QGS must not import QNC crates.

## Risks

- Current payload binding is a token/readback-friendly record, not a
  displayable swapchain image.
- `RgbaU16` may not be a direct swapchain format; a presenter copy/conversion
  path may be needed.
- Visual correctness and real presentation are separate forms of evidence.
- Wayland surface/window creation can accidentally drift into UI ownership if
  the boundary is not kept narrow.
- Real display backends need synchronization and backpressure policy before
  realtime playback is attempted.
- DRM/KMS may be attractive for appliance output but is too high-risk for the
  first journalist-laptop milestone.
- `winit`/`wgpu` can help prototype but may obscure evidence QGS needs to own
  explicitly.

## Recommended Next Integration Block

M2 Integration Block E - Screenshot/File Presenter Visual Diagnostic.

Suggested scope:

- consume one existing prepared `ProcessedGpuFrame` payload
- write a bounded diagnostic image from current RGBA-u16 output
- produce deterministic metadata: source URI, frame index, format, dimensions,
  checksum, and private-path exposure status
- optionally compare against a stored/generated expected checksum when a stable
  fixture exists
- keep real display output `NotImplemented`
- keep `FramePresentedRealBackend`, `VisualVerified`, and `RealtimeVerified`
  unclaimed unless evidence actually supports them

After that, the next real display block should be a minimal Wayland/Vulkan
presenter prototype that can create a surface and swapchain without tying the
presenter boundary to session control or QNC UI.

## Explicit Non-Claims

This audit does not implement or claim:

- real display output
- Wayland/X11/DRM/KMS output
- swapchain creation
- real user-visible presentation
- visual verification
- realtime playback
- A/V sync
- QNC UI integration
- export/render
- a monolithic player runtime

`TestPresenter` remains test-boundary evidence only. Monitor projection remains
passive. Real display output remains `NotImplemented`, and `VisualVerified` and
`RealtimeVerified` remain not claimed.

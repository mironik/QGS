# M2 Integration Block B - QGS Presenter / Monitor Boundary

Integration Block B connects existing QGS prepared video payload facts to a
backend-neutral presenter/monitor boundary.

This is not realtime playback, A/V sync, QNC UI, export/render, full playout, or
a production display output claim. It proves the boundary shape:

```text
prepared video payload
  -> presenter boundary submission
  -> explicit presenter evidence
  -> monitor projection update
```

## Relationship To Step 22A

Step 22A identified the QNC `qnc-video-output` and `qnc-monitor` responsibilities
that QGS must replace or feed:

- QGS owns prepared payloads and backend presenter evidence.
- QNC monitor/timeline modules remain passive observers.
- QNC UI does not own display/presenter readiness.
- Real display presenter output remains a gap. The QNC OS target is
  Wayland/Vulkan; DRM/KMS is a possible later direct/appliance output path, and
  X11 is legacy/non-target for QNC OS.

Block B keeps that ownership line. It provides monitor projection facts from QGS
backend evidence without importing QNC crates or implementing a QNC UI.

## Relationship To Integration Block A

Integration Block A added the in-process session runtime control surface:
bounded commands, generation checks, passive views, and public transcripts.

Block B uses that surface only enough to create a prepared-session context. It
then selects one deterministic prepared proxy-video payload descriptor and sends
that descriptor through the presenter boundary. It does not add a realtime loop
or make Play perform device work.

## Presenter Boundary Model

The new presenter/monitor records are backend-neutral:

- `QgsPresenterPayloadDescriptor`
- `QgsPresenterSubmission`
- `QgsPresenterSubmissionResult`
- `QgsPresenterEvidence`
- `QgsMonitorFrameDescriptor`
- `QgsMonitorProjectionUpdate`

The descriptor records public source URI, source frame, source mode through the
payload binding, payload kind, payload format, processing backend, dimensions,
and whether any private path was exposed.

For this block the concrete presenter kind is `TestPresenter`.

## Evidence Distinctions

Block B keeps these states separate:

- prepared payload exists
- submitted to presenter boundary
- test presenter accepted
- real display evidence
- visual verification

Test presenter acceptance means only that the QGS presenter boundary was called
with a valid prepared payload descriptor and returned structured test evidence.
It does not mean a user-visible frame reached a real display.

Current evidence:

- presenter/monitor boundary: `TestBoundaryEvidence`
- real display output: `NotImplemented`
- `VisualVerified`: not claimed
- `RealtimeVerified`: not claimed

## Monitor Projection Update

`QgsQncMonitorProjection` now has explicit presenter/monitor facts in addition
to the earlier Phase 27 placeholder fields:

- prepared descriptor present
- submitted to presenter
- presenter evidence kind
- test-boundary presented
- presented as real display
- real display evidence
- visual verified
- private path exposed

For test presenter evidence:

- submitted to presenter: yes
- presenter evidence kind: `TestPresenterAccepted`
- test-boundary presented: yes
- presented as real display: no
- real display evidence: none
- visual verified: no

## QGS-Test Command

```bash
cargo run -q -p qgs-test -- \
  --qgs-presenter-monitor-boundary <original-mxf> <proxy-mp4>
```

The command:

1. Builds the Phase 22 descriptor and `QgsInputPlan`.
2. Runs enough Integration Block A session setup to reach prepared runtime
   context.
3. Selects deterministic proxy source frame 0.
4. Binds that proxy frame to a real processed GPU payload token.
5. Submits the payload descriptor to the test presenter boundary.
6. Projects the result into monitor facts.

The command does not read the full original MXF payload. Original MXF media
remains authoritative for the overall QGS media model, but this command is a
video presenter/monitor boundary check, not an audio payload test.

## Acceptance Results

For Sony FX6 sample 002 / Mironik 1560:

- public source URI: `qnc://local/media/proxy/Mironik-1560`
- selected frame: 0
- payload kind: `ProcessedGpuFrame`
- payload format: `RgbaU16`
- payload backend: `VaapiCpuNv12Vulkan`
- payload dimensions: visible 1920x1080, coded 1920x1088
- presenter kind: `TestPresenter`
- submitted to presenter: yes
- presenter evidence kind: `TestPresenterAccepted`
- test presenter accepted: yes
- monitor projection prepared descriptor: yes
- real display evidence: none
- visual verified: no
- private path exposed: no

For Mironik 2002, the same boundary succeeds over frame 0 using public source
URI `qnc://local/media/proxy/Mironik-2002`. The command remains large-file safe:
it does not slurp the original MXF to prove a video presenter boundary.

## Non-Claims

Not implemented:

- real display backend
- Wayland/DRM/KMS/X11/swapchain output
- real user-visible presentation
- visual comparison or correctness verification
- realtime scheduler
- A/V sync
- QNC UI integration
- export/render

`FramePresented` is not claimed as real display presentation. Test presenter
evidence remains test-boundary evidence only.

## Next Integration Block

Integration Block D audits real display presenter backend options. It keeps the
presenter boundary independent, recommends a screenshot/file visual diagnostic
before a minimal Wayland/Vulkan presenter prototype, treats DRM/KMS as a later
direct-output option, keeps X11 as legacy/non-target for QNC OS, and still keeps
`VisualVerified` separate from merely submitting a frame to a presenter.

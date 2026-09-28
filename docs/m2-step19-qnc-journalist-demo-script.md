# QGS M2 Step 19 QNC Journalist Demo Script

M2 Step 19 turns the current QGS proof chain into an internal product-facing
demo script for QNC Journalist. It is meant for explaining the workflow to
broadcast stakeholders without claiming that QNC OS is already a finished
editor.

## What This Demonstrates

The demo shows a realistic QNC Journalist workflow on modest Linux hardware:

- camera media ingest from a real Sony FX6 original/proxy pair
- original/proxy relationship established by the existing media inspection path
- a `journalist-50i-preview` broadcast preview profile
- bounded decode, CPU transfer, GPU processing, and presentation accounting
- deterministic frame/time selection for a small news cut
- an export-plan stub that keeps the original MXF available for finishing

The current profile is a 50i-compatible broadcast journalist preview. It uses
a 25 frame-period/s progressive processing workload over a 1080p50 camera proxy
source. It is not a full 50i renderer.

## What Is Not Claimed

This demo does not claim:

- finished QNC UI
- full NLE behavior
- real export or render output
- audio playback or sync
- full 50i field rendering
- field cadence
- interlaced output
- deinterlacing
- stable full 1080p50 production playback on Haswell/i965
- VA/Vulkan zero-copy

The value of this milestone is product shape: it shows that the technical
pieces can already be presented as a coherent journalist/news workflow.

## Suggested Command

Use placeholder paths in demos and notes:

```text
cargo run -q -p qgs-test -- --qnc-journalist-demo <original-mxf> <proxy-mp4>
```

The command prints privacy-safe media labels. It must not expose private camera
serial metadata, private filesystem paths, private UMIDs, or production
timestamps in committed documentation.

## Demo Narrative

1. Introduce the media.

   "This is a real Sony FX6 camera-original/proxy pair. The original MXF remains
   available for finishing, while the proxy MP4 is used for responsive
   journalist preview."

2. Explain the proxy.

   "QGS recognizes the proxy as H.264 High, 8-bit 4:2:0, 1920 x 1080 visible,
   50p source media. The source has 106 presentation frames."

3. Explain the profile.

   "The demo uses `journalist-50i-preview`. This is a 50i-compatible broadcast
   journalist preview profile. It presents every second source frame as a 25
   frame-period/s progressive preview workload. It is not a full interlaced
   renderer."

4. Run the command.

   Point out that QGS performs the product-shaped flow:

   ```text
   camera media
     -> original/proxy association
     -> journalist-50i-preview profile
     -> bounded decode/process/presentation
     -> frame/time selection accounting
     -> export-plan stub
   ```

5. Read the accounting.

   Highlight that intentional source-frame skips are profile behavior, not
   playback failure. They are reported separately from lateness drops.

6. Show the news cut.

   The demo selects a deterministic range from about 0.400 s to 1.600 s and
   reports the corresponding preview/source frame accounting.

7. Show the export plan.

   "No file is rendered in this milestone. The export plan states that preview
   came from the proxy and finishing media remains the original MXF."

8. Close with the product point.

   "This is the first QNC OS Journalist-shaped proof: real camera media, proxy
   preview, bounded resources, deterministic accounting, and a clear path from
   newsroom preview to future original-media finishing."

## Expected Output Summary

The important fields to point out are:

- association result for the Sony FX6 original/proxy pair
- preview profile: `journalist-50i-preview`
- source: 1080p50 camera proxy
- broadcast target: 1080i50-compatible news preview
- processing workload: 25 frame periods/s
- true interlaced output: not implemented in this milestone
- source frames: 106
- selected preview frames: 53
- intentional source-frame skips: 53
- lateness drops: 0
- duplicated frames: 0
- GPU submissions/completions: 53 / 53
- export plan status: planned only, not rendered
- finishing media: original MXF available

Representative measured result on the Haswell/i965 system:

- 53 selected preview frames
- 53 presented
- 0 lateness drops
- 0 duplicated frames
- bounded CPU NV12 pool
- bounded GPU FrameSlots

The exact wall-clock timing may vary between runs. The important demo result is
that the selected broadcast-preview workload completed without lateness drops or
duplicates on the tested Haswell machine.

## Stakeholder Value

This demo supports the QNC Journalist value proposition in plain terms:

- QNC OS can target ordinary journalist hardware, not only expensive edit
  workstations.
- The workflow uses real camera-generated Sony FX6 media rather than a toy
  fixture.
- QGS distinguishes intentional broadcast-preview frame selection from failure
  drops.
- Camera-original media stays available for finishing.
- The path is Linux-first and QNC OS oriented.
- The architecture suggests a credible route toward low-cost newsroom editing
  while preserving a professional finishing path.

This should be presented carefully: it is promising, but it is not a finished
editing product yet.

## Technical Basis

The demo stands on the current QGS proof chain:

- Step 12 proved original/proxy association and clean proxy ingest for Sony FX6
  sample 002.
- Step 17 proved the bounded CPU-bridge NV12 fallback path and compact Vulkan
  NV12 processing.
- Step 18A proved the `journalist-50i-preview` profile: every second 50p source
  frame selected, 53 presented, 0 lateness drops, and 0 duplicates.
- Step 18B packaged those pieces into a product-shaped QNC Journalist command
  with frame/time selection accounting and an export-plan stub.

## Limitations

- No GUI shell.
- No timeline engine.
- No audio playback or audio-master clock.
- No real display output.
- No export/render path.
- No graphics/title system.
- No automatic proxy/original switching policy.
- No full 50i field renderer.
- No full 1080p50 production playback guarantee on Haswell/i965.
- No VA/Vulkan zero-copy.

## Next Milestones

Useful next product and platform milestones:

- real QNC Journalist UI shell
- audio playback and sync
- real export/render path
- graphics/title path
- modern-hardware VA/Vulkan zero-copy validation
- full 50i field renderer if QNC Journalist needs true interlaced output
- full 1080p50 production playback validation on modern Intel/AMD hardware


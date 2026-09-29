# M2 Integration Block E — Screenshot/File Presenter Visual Diagnostic

Block E adds the first file-based visual diagnostic boundary for the QGS
Broadcast Player Runtime.

This is not a real display backend. It is not the QNC OS Wayland/Vulkan
presenter, not an X11 target, not realtime playback, not A/V sync, and not QNC
UI integration.

## Purpose

Integration Block D selected the display backend direction:

- diagnostic path first: screenshot/file presenter visual diagnostic
- primary QNC OS real display target later: Wayland + Vulkan
- optional future appliance/direct-output target: DRM/KMS + Vulkan
- X11 is legacy Linux compatibility only and is not a QNC OS display target

Block E creates the diagnostic file-presenter boundary that can describe a
prepared video payload without claiming real display output.

## Implemented Boundary

The new qgs-test command is:

```bash
cargo run -q -p qgs-test -- --qgs-file-presenter-diagnostic <original-mxf> <proxy-mp4>
```

An optional output directory may be supplied with:

```bash
--output-dir <path>
```

If no output directory is supplied, the command writes under:

```text
target/qgs-visual-diagnostics
```

For the current `ProxyPreview` path, QGS already has a prepared proxy video
payload descriptor:

- video source: proxy MP4
- audio source: original MXF
- proxy AAC: not used
- preview profile: `journalist-50i-preview`
- payload kind: processed GPU frame token
- payload format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`

At this boundary the runtime has a processed GPU payload token/descriptor, not a
CPU-readable frame pixel buffer. Therefore Block E writes a descriptor manifest,
not a PNG/PPM image. No fake pixels are generated.

## Artifact Model

The file presenter can represent:

- descriptor manifest written
- image file written, if future payload boundaries expose real readable pixels
- placeholder/manifest-only result when pixels are unavailable

Current evidence level:

```text
FilePresenterManifestWritten
```

Current image artifact status:

```text
image unavailable: prepared payload descriptor does not expose CPU-readable frame pixels
```

The manifest uses public runtime identity, for example `qnc://...`, and stores a
public-safe artifact path. It does not include raw local MXF/MP4 filesystem
paths.

## Monitor Projection

The monitor projection now distinguishes:

- prepared descriptor present
- submitted to file presenter
- file-presenter manifest evidence
- no test-presenter `FramePresented`
- no real display `FramePresented`
- no real display evidence
- no visual verification

A file manifest is useful diagnostic evidence, but it is not user-visible
display presentation.

## Sony FX6 Sample 002 Expected Result

For sample 002, the command was run as:

```bash
cargo run -q -p qgs-test -- --qgs-file-presenter-diagnostic \
  "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560.MXF" \
  "/home/miro/QGS-media-tests/sony-fx6/sample-002/Mironik 1560S03.MP4"
```

Observed result:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- public source URI: `qnc://local/media/proxy/Mironik-1560`
- selected frame: `0`
- selected preview frame: `0`
- payload kind: `ProcessedGpuFrame`
- payload format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`
- dimensions: 1920x1080 visible, 1920x1088 coded
- artifact kind: descriptor manifest
- artifact path:
  `target/qgs-visual-diagnostics/Mironik-1560-frame000000-proxy-preview-manifest.json`
- image artifact available: no
- manifest written: yes
- evidence level: `FilePresenterManifestWritten`
- real display output: no
- visual verified: no
- realtime playback: no
- A/V sync: no
- private path exposed: no

## Mironik 2002

The same command was run against the local Mironik 2002 original/proxy pair:

```bash
cargo run -q -p qgs-test -- --qgs-file-presenter-diagnostic \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002.MXF" \
  "/home/miro/QGS-media-tests/sony-fx6/mironik-2002/Mironik 2002S03.MP4"
```

Observed result:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- public source URI: `qnc://local/media/proxy/Mironik-2002`
- selected frame: `0`
- selected preview frame: `0`
- payload kind: `ProcessedGpuFrame`
- payload format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`
- dimensions: 1920x1080 visible, 1920x1088 coded
- artifact kind: descriptor manifest
- artifact path:
  `target/qgs-visual-diagnostics/Mironik-2002-frame000000-proxy-preview-manifest.json`
- image artifact available: no
- manifest written: yes
- evidence level: `FilePresenterManifestWritten`
- real display output: no
- visual verified: no
- realtime playback: no
- A/V sync: no
- private path exposed: no

The Mironik 2002 result is still descriptor-only because the current payload
boundary exposes a processed GPU token/descriptor, not readable pixels.

## Verification Matrix Update

Step 20Q now includes:

```text
file presenter visual diagnostic: FilePresenterManifestWritten
```

This remains below:

- `VisualVerified`
- real display output
- realtime playback
- hardware display validation

## Not Implemented

Block E does not implement:

- real display output
- Wayland/Vulkan presenter
- DRM/KMS presenter
- X11 presenter
- Vulkan swapchain presentation
- screenshot comparison
- visual verification
- realtime scheduling
- A/V sync
- QNC UI integration
- export/render

## Next Block

The recommended next display milestone is:

```text
M2 Integration Block F — Wayland + Vulkan Presenter Boundary Prototype
```

That later block should remain separate from this file-presenter diagnostic and
should only claim real display evidence when a real presenter provides it.

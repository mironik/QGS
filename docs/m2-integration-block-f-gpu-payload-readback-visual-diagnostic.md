# M2 Integration Block F — GPU Payload Readback / Visual Diagnostic Bridge

Block F adds a bounded visual diagnostic bridge from the existing prepared proxy
GPU payload path to a CPU-readable file artifact.

This is not a Wayland/Vulkan presenter. It is not X11. It is not realtime
playback, A/V sync, QNC UI integration, export/render, or real display output.

## Relationship to Block E

Block E introduced the file presenter diagnostic and wrote descriptor manifests
because the normal prepared payload boundary exposed a processed GPU token, not
CPU-readable pixels.

Block F answers the next diagnostic question: can QGS obtain real pixels from
the existing GPU processing path when an explicit validation readback is
requested?

For the current proxy preview path, the answer is yes.

## Current Payload Shape

The accepted path remains:

- source mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF
- proxy AAC: not used
- preview profile: `journalist-50i-preview`
- payload kind: `ProcessedGpuFrame`
- payload format: `RgbaU16`
- backend path: `VaapiCpuNv12Vulkan`

The normal Broadcast Player Runtime payload remains a GPU payload token. Block F
uses the existing Vulkan validation readback path for one bounded diagnostic
frame and does not change the runtime truth.

## Readback Implementation

The diagnostic command uses:

```text
proxy MP4
 -> qgs-mp4 H.264 access units
 -> qgs-vaapi decode
 -> VA -> CPU NV12 transfer
 -> qgs-vulkan NV12 processing with validation_readback enabled
 -> CPU-readable RgbaU16 diagnostic readback
 -> PPM P6 RGB8 file
```

The output image conversion is:

```text
RgbaU16 high byte -> RGB8; alpha discarded
```

This conversion is deterministic and dependency-free. It is a diagnostic
visualization, not color-accurate broadcast output.

## qgs-test Command

```bash
cargo run -q -p qgs-test -- --qgs-gpu-payload-readback-diagnostic <original-mxf> <proxy-mp4>
```

Optional output directory:

```bash
--output-dir <path>
```

Default output directory:

```text
target/qgs-visual-diagnostics
```

The command writes:

- a `.ppm` image artifact
- a sidecar JSON manifest

The manifest uses public runtime identities and does not expose raw private
local media paths.

## Sony FX6 Sample 002 Result

Command:

```bash
cargo run -q -p qgs-test -- --qgs-gpu-payload-readback-diagnostic \
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
- readback attempted: yes
- readback available: yes
- readback format: `RgbaU16`
- output artifact format: PPM P6 RGB8
- image artifact:
  `target/qgs-visual-diagnostics/Mironik-1560-frame000000-proxy-preview-readback.ppm`
- manifest artifact:
  `target/qgs-visual-diagnostics/Mironik-1560-frame000000-proxy-preview-readback-manifest.json`
- image bytes written: `6220817`
- GPU readback payload bytes: `16588800`
- GPU readback checksum: `0x493af58fdbe269f3`
- evidence level: `FilePresenterImageWritten`
- real display output: no
- `VisualVerified`: no
- realtime playback: no
- A/V sync: no
- private path exposed: no

## Mironik 2002 Result

Command:

```bash
cargo run -q -p qgs-test -- --qgs-gpu-payload-readback-diagnostic \
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
- readback attempted: yes
- readback available: yes
- readback format: `RgbaU16`
- output artifact format: PPM P6 RGB8
- image artifact:
  `target/qgs-visual-diagnostics/Mironik-2002-frame000000-proxy-preview-readback.ppm`
- manifest artifact:
  `target/qgs-visual-diagnostics/Mironik-2002-frame000000-proxy-preview-readback-manifest.json`
- image bytes written: `6220817`
- GPU readback payload bytes: `16588800`
- GPU readback checksum: `0xb019c3c44e11caf9`
- evidence level: `FilePresenterImageWritten`
- real display output: no
- `VisualVerified`: no
- realtime playback: no
- A/V sync: no
- private path exposed: no

## Evidence Level

Step 20Q now records:

```text
GPU payload readback visual diagnostic: FilePresenterImageWritten
```

This means QGS wrote a diagnostic image from real readback pixels. It does not
mean:

- real display output
- real presenter evidence
- visual correctness comparison
- `VisualVerified`
- realtime playback
- A/V sync

## Why This Is Not VisualVerified

The PPM is a diagnostic artifact. No image comparison, reference-frame
comparison, human visual QA protocol, or display presentation evidence is part
of this block.

`VisualVerified` should only be claimed by a future milestone that defines and
runs an explicit visual verification process.

## Not Implemented

Block F does not implement:

- Wayland/Vulkan presenter
- X11 presenter
- DRM/KMS presenter
- swapchain presentation
- real display output
- real `FramePresented` backend evidence
- visual comparison
- realtime scheduling
- A/V sync
- QNC UI integration
- export/render

## Next Recommended Block

Because bounded readback image writing succeeded, the next display milestone can
move toward:

```text
M2 Integration Block G — Wayland + Vulkan Presenter Boundary Prototype
```

That later block should remain separate from this file/readback diagnostic and
must not claim real display output until a real presenter provides evidence.

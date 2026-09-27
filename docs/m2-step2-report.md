# QGS M2 Step 2 Report

## VA Rust Dependency

`qgs-vaapi` uses `libva 0.1.4` with `default-features = false` and the `linked`
feature. It was selected because it provides safe Rust wrappers for opening a
DRM render-node VA display and querying profiles, entrypoints, config
attributes, and image formats. No QGS-owned unsafe code was needed.

## qgs-vaapi Architecture

`qgs-vaapi` is a new backend crate responsible only for VA-API decode
capability discovery and translation into QGS-owned `VideoDecodeCapability`
entries. VA-API types do not escape the crate.

`qgsd` composes the existing Vulkan device/resource backend with the VA-API
video capability backend. `qgs-core` and `qgs-protocol` remain backend-neutral.

## DRM Render-Node Matching

`qgs-vaapi` enumerates `/dev/dri/renderD*` and reads sysfs vendor/device IDs
from `/sys/class/drm/<render-node>/device/{vendor,device}`. It matches those
IDs to QGS `DeviceDesc` vendor/device identity and opens the matched render
node. It does not rely on Vulkan or VA enumeration order.

Current mapping:

- Intel HD Graphics 4600: `/dev/dri/renderD128`, vendor/device `0x8086/0x0416`,
  driver `i915`
- NVIDIA GTX 950M: `/dev/dri/renderD129`, vendor/device `0x10de/0x139a`,
  driver `nouveau`

## VA Drivers Discovered

Installed VA components:

- `vainfo 2.22.0+ds1-2build1`
- `libva-dev 2.23.0-1ubuntu1`
- `libva2 2.23.0-1ubuntu1`
- `libva-drm2 2.23.0-1ubuntu1`
- `i965-va-driver 2.4.1+dfsg1-2build1`

Driver libraries discovered:

- Intel: `/usr/lib/x86_64-linux-gnu/dri/i965_drv_video.so`
- NVIDIA/nouveau: `/usr/lib/x86_64-linux-gnu/dri/nouveau_drv_video.so`

`mesa-va-drivers` is not installed, but nouveau VA support is available through
the present Gallium driver library.

## Profile And Entrypoint Translation

Only `VAEntrypointVLD` is translated as decode. Encode and VideoProc
entrypoints are ignored.

Implemented profile mappings:

- `VAProfileH264ConstrainedBaseline` -> H.264 Baseline
- `VAProfileH264Main` -> H.264 Main
- `VAProfileH264High` -> H.264 High
- `VAProfileMPEG2Main` -> MPEG-2 Main

Unsupported profiles are ignored rather than promoted to QGS support.

## Surface-Format Translation

`VAConfigAttribRTFormat` is combined with VA image-format FourCCs:

- `VA_RT_FORMAT_YUV420` + `NV12` -> `VideoSurfaceFormat::Nv12`
- `VA_RT_FORMAT_YUV420_10` + `P010` -> `VideoSurfaceFormat::P010`
- `VA_RT_FORMAT_YUV422` + `YUY2` or `UYVY` -> `VideoSurfaceFormat::Yuv422_8`
- `VA_RT_FORMAT_YUV422_10` + `Y210` -> `VideoSurfaceFormat::Yuv422_10`

The current Intel decode profiles expose YUV420/NV12. No 4:2:2 or 10-bit
decode output was reported for the target workflows.

## Intel Capability Result

Probe:

```text
LIBVA_DRIVER_NAME=i965 vainfo --display drm --device /dev/dri/renderD128
```

Driver:

- Intel i965 driver for Intel(R) Haswell Mobile, `2.4.1`

QGS-reported decode capabilities:

- MPEG-2 Main, 8-bit, 4:2:0, NV12
- H.264 Baseline, 8-bit, 4:2:0, NV12
- H.264 Main, 8-bit, 4:2:0, NV12
- H.264 High, 8-bit, 4:2:0, NV12

The i965 driver did not expose exact max picture width/height through the
queried config attributes, so QGS reports the bounded M2 video model ceiling
of 8192 x 8192 in those fields and records this as a VA reporting limitation.

## NVIDIA Capability Result

Probe:

```text
vainfo --display drm --device /dev/dri/renderD129
```

Driver:

- Mesa Gallium driver `26.0.8-1ubuntu0.3` for NV117

The nouveau VA driver opened successfully but reported only
`VAProfileNone : VAEntrypointVideoProc`. QGS reports an empty decode capability
list for the NVIDIA GTX 950M.

## Vulkan Video Probe

`vulkaninfo` is installed. `vulkaninfo --summary` and an extension-name search
did not report `VK_KHR_video_queue`, `VK_KHR_video_decode_queue`, or codec
decode extensions for the current devices. The full `vulkaninfo` text contains
generic Vulkan enum names such as `QUEUE_VIDEO_DECODE_BIT_KHR`, but those are
not device support claims. Vulkan Video is therefore not a practical first
decode backend on this machine.

## Reference Workflow Results

H.264 8-bit 4:2:0:

- Intel: supported through H.264 Baseline/Main/High VLD with NV12 output
- NVIDIA/nouveau: not reported

MPEG-2 / XDCAM-relevant:

- Intel: MPEG-2 Main 8-bit 4:2:0 is reported
- MPEG-2 4:2:2 was not reported by the Intel i965 VA probe
- NVIDIA/nouveau: not reported

H.264 10-bit 4:2:2 / XAVC-relevant:

- Intel: not reported
- NVIDIA/nouveau: not reported

## VA Reporting Limitations

The VA probe distinguishes decode entrypoints from encode and VideoProc. It
also reports render-target format classes. It does not cleanly report
progressive/interlaced distinctions for these profiles in this discovery path,
so QGS does not claim interlaced support from VA in Step 2.

For the current Intel i965 driver, exact maximum coded dimensions were not
reported through the queried config attributes. QGS keeps payloads bounded and
uses the existing M2 dimension ceiling while documenting that limitation.

## Tests

`cargo test --workspace` passed.

Total: 145 tests passed.

- `qgs-core`: 26 passed
- `qgs-linux`: 4 passed
- `qgs-protocol`: 105 passed
- `qgs-vaapi`: 8 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Real QGS Capability Query

`qgsd` and `qgs-test --video-capabilities-only` were run as separate processes.
The query returned four Intel decode entries and empty video decode lists for
NVIDIA/nouveau and llvmpipe.

## Unsafe Inventory

No new QGS-owned unsafe code was added.

`qgs-protocol`, `qgs-core`, `qgs-linux`, `qgs-vaapi`, `qgsd`, and `qgs-test`
remain `#![forbid(unsafe_code)]`. The existing audited qgs-vulkan unsafe
inventory is unchanged.

## Commit And Push Verification

This report is included in the M2 Step 2 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after commit and push.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Architectural Concerns

No decode implementation was added. No bitstreams are parsed, no VA decoder
contexts or surfaces are allocated for decode, and no MXF/FFmpeg/software
fallback work was introduced.

The most important limitation is that Intel i965 reports useful decode
profile/format support, but not exact maximum coded dimensions through the
queried attributes. QGS keeps the response bounded and records the limitation
rather than inventing broader support.

## Recommendation For M2 Step 3

Use VA-API as the first real decode backend.

This recommendation is based on the current probes: Intel HD Graphics 4600 has
working VA-API H.264 and MPEG-2 VLD decode capability through i965, while
Vulkan Video support is not available on this hardware path and nouveau exposes
no VA decode profiles. M2 Step 3 should start with Intel VA-API decode for
H.264 8-bit 4:2:0, then evaluate MPEG-2 Main, while keeping NVIDIA/nouveau as a
known no-decode result for this machine.

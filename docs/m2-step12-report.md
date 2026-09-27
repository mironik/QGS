# M2 Step 12 Report: Camera Original/Proxy Association and Proxy Playback Proof

## Summary

M2 Step 12 adds a bounded `qgs-mp4` camera-proxy frontend and proves a clean
Sony FX6 original/proxy pair through:

```text
original MXF -> qgs-mxf -> qgs-codec-h264 -> software fallback
proxy MP4   -> qgs-mp4 -> qgs-codec-h264 -> Intel VA hardware decode
proxy frames -> reusable qgs-vulkan GPU processing proof
```

The frozen VA -> Vulkan zero-copy path was not used.

## qgs-mp4 Architecture

`qgs-mp4` is a safe Rust crate with `#![forbid(unsafe_code)]`. It implements
the bounded ISO BMFF subset needed by the camera proxy files: track discovery,
`avc1`/`avcC`, sample timing, sample-to-chunk mapping, chunk offsets, sample
sizes, sync samples, and bounded compressed sample extraction.

It does not decode pixels, parse MXF, call VA-API, call Vulkan, or introduce
Qnc timeline concepts.

## MP4 Dependency Decision

`qgs-mp4` uses the Rust `mp4` crate, version 0.14.0, for ISO BMFF box/sample
table reading. QGS still owns the bounded media-source model, track filtering,
sample validation, AVC configuration handling, strict AVC normalization, timing
identity, and health classification used by the proxy proof.

The direct dependency tree is limited to `mp4` plus its ordinary Rust parsing
support crates. No FFmpeg/libavformat MP4 demux path was added, and qgs-mxf
remains the MXF frontend.

FFmpeg/ffprobe were used only as development diagnostics.

## CASE 001 Damage Finding

Sony FX6 sample 001 proxy is not used for all-frame Step 12 acceptance.

Observed:

- container declares 106 video packets
- video sample 97 byte range matches ffprobe
- sample 97 begins with a zero-length length-prefixed AVC NAL
- QGS strict AVC normalization rejects the sample
- strict `ffmpeg -xerror` also fails
- frame-hash diagnostics produce 96 decoded frames

Final classification: damaged/unrecoverable for complete proxy playback.

## Strict Zero-Length NAL Policy

QGS does not treat a zero-length AVC NAL as padding. A normalized access unit
with no valid slice remains malformed. A synthetic regression covers this
without committing camera packet bytes.

## Media Health Distinction

Step 12 records the future distinction between:

- `Valid`
- `ValidButUnsupported`
- `DamagedRecoverable`
- `DamagedUnrecoverable`
- `FatalContainerError`

Only the minimum diagnostic semantics required for Step 12 were introduced.

## CASE 002 Provenance

External local corpus only, not committed.

- original MXF SHA-256:
  `52a82d3527717096892a78bfd62f62f864e891fc4321e09ad7ed0856a7b9f24e`
- Sony XML SHA-256:
  `985d1f373616801601b2da2b3c1458d09aac2e5a6dbf12ae374bd3ee16c904e6`
- proxy MP4 SHA-256:
  `fa7b646f7dc84bee84744dbaee89924b7f94982405c2fc3ffea2936618f73007`

Strict FFmpeg proxy decode from the local copy exited successfully.

## CASE 002 Proxy Technical Properties

QGS/diagnostics observed:

- container: MP4 / ISO BMFF family
- major brand: XAVC
- video: H.264 High, 8-bit, 4:2:0
- dimensions: 1920 x 1080
- rate: 50/1
- time base: 1/50000
- duration: 106000 time units / 2.12 seconds
- video samples/read frames/read packets: 106 / 106 / 106
- `avcC`: 4-byte NAL length fields, 1 SPS, 2 PPS
- audio: AAC, 48 kHz, 2 channels
- data: additional data track discovered

## Original MXF Result

`qgs-mxf` parses the original as professional H.264 High 4:2:2 10-bit MXF
media with 106 edit units at 50/1. The original remains a software-fallback
decode path on this Haswell machine because Intel i965 does not advertise
H.264 High 4:2:2 10-bit hardware decode.

## Association Evidence

Filename similarity is not used.

Association evidence:

- Sony XML sidecar reports proxy/substream metadata
- XML duration is 106 edit units
- original MXF edit-unit count is 106
- proxy presentation sample count is 106
- original and proxy rates are 50/1
- XML dimensions match the proxy/original frame dimensions

Association confidence: strong metadata plus timing evidence.

## Timing and Frame Map

QGS observes one proxy presentation sample per original edit unit for sample
002. DTS/PTS/composition timing stays rational and container-owned; no Qnc
timeline identity is introduced.

Proof positions:

- first presentation frame
- middle frame 53
- final frame

## GOP and Random Access

The proxy contains I and P pictures. QGS observes random-access positions at
samples 0, 48, and 96. The proxy and original do not need identical GOP
structures.

For the proxy software reference path, random access to presentation frame 53
starts from sample 48 and matches the sequential frame-53 checksum.

## Intel Capability and Decode Result

Intel HD Graphics 4600 / i965 advertises support for H.264 High 8-bit 4:2:0.

The proxy decodes through qgs-vaapi:

- decoder creation: pass
- `avcC` SPS/PPS initialization and normalized access units: pass
- I/P picture handling: pass
- `frame_num` wrap and short-term reference ordering: fixed
- DPB-released surface reuse: fixed
- flush/drain: pass
- decoded presentation frames: 106 / 106

Development observation, not a benchmark:

- hardware decode proof with current validation/export diagnostics enabled:
  91.710 s, 1.16 fps, 0.02x realtime at 50 fps

This number is dominated by current per-frame VA validation/export diagnostic
work and is not a clean production throughput measurement.

## Original Software Decode Comparison

Development observation, not a benchmark:

- original software decode path: 106 frames, about 30.358 s, 3.49 fps
- proxy software reference decode: 106 frames, about 11.302 s, 9.38 fps

The production-relevant proxy decode direction is Intel VA hardware decode, but
the current qgs-vaapi diagnostic readback path must be separated before a fair
throughput observation.

## Proxy to GPU Proof

The proof does not use VA -> Vulkan zero-copy.

Selected proxy frames are decoded/validated through the proxy path, converted
into a CPU-backed proof surface, and submitted to the reusable GPU processor.
Results pass on:

- Intel HD Graphics 4600
- NVIDIA GTX 950M / NVK

Frame 53 sequential and random-access GPU checksums match:

- `0x0c8466c6a8727f05`

The proof also validates first and final proxy frames with max CPU/GPU delta 1.

## Codec and VA Fixes Exposed by Step 12

The clean proxy crossed H.264 `frame_num` wrap. QGS now uses short-term PicNum
relative to the current picture for default P-slice reference ordering,
reference-list modification lookup, and sliding-window eviction.

The qgs-vaapi backend now recycles surfaces released from the H.264 DPB when no
client resource still owns them. The proxy proof tool releases decoded proxy
surfaces after counting them so the proof does not retain an entire clip of VA
surfaces unnecessarily.

IDR reset now drains pending output pictures before clearing prior DPB state
when `no_output_of_prior_pics_flag` permits output.

## Step 11 Regression

The reusable GPU processor remains the path used for proxy GPU proof. The
existing Step 11 frame-slot architecture was not replaced or bypassed.

## Tests and Results

Focused tests run during implementation:

- `cargo test -p qgs-mp4`: 5 passed
- `cargo test -p qgs-codec-h264`: 8 passed
- `cargo check -p qgs-vaapi -p qgs-test`: passed

External acceptance:

- sample 001 damaged proxy diagnostic: passed
- sample 002 strict FFmpeg proxy check: passed
- sample 002 QGS original/proxy proof: passed

Full workspace quality gates are recorded with the final commit verification.

## Unsafe Inventory

No new unsafe Rust was added. `qgs-mp4` forbids unsafe code. Vulkan unsafe
inventory is unchanged.

## Privacy Handling

Committed documentation uses privacy-safe sample labels only. It omits original
filenames, local paths, camera serial numbers, full private UMIDs, and private
production timestamps. Camera media and XML sidecars remain outside the
repository.

## Limitations

- qgs-mp4 is a bounded proxy-ingest subset, not a complete MP4/MOV
  implementation.
- Damaged-media recovery is diagnostic only; no broad concealment/recovery
  framework is implemented.
- Proxy audio is discovered but not decoded.
- Proxy data tracks are discovered but not interpreted.
- VA -> Vulkan zero-copy remains frozen.
- The current hardware decode timing is not a fair proxy playback performance
  number while qgs-vaapi performs per-frame validation/export diagnostics.

## Recommendation

Next milestone should separate qgs-vaapi validation/export diagnostics from the
normal decode path and then design a proxy-aware editing architecture around
explicit media-variant association, timing identity, and bounded decode/GPU
frame scheduling. Do not add Qnc proxy switching until the clean hardware
decode throughput path is measured without diagnostic readback.

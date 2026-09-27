# QGS M2 Step 7 Report

## Summary

M2 Step 7 introduces the first QGS-owned MXF demux/index foundation. It adds a
safe `qgs-mxf` crate, generated synthetic MXF fixtures, and a local `qgs-test`
inspection mode that proves:

```text
MXF
  -> qgs-mxf KLV/essence index
  -> bounded H.264 access units
  -> qgs-codec-h264 classification
```

No daemon file-opening protocol was added. The Haswell VA -> Vulkan zero-copy
path remains frozen.

## MXF Rust Dependency Research

Crates inspected:

- `st377-1 0.3.0`: broad ST 377-1 parsing, MIT OR Apache-2.0, but requires
  Rust 1.95 and would pull in more MXF surface than needed for this milestone.
- `smpte-klv 0.1.1`: ST 336 KLV/BER/local-set utilities, MIT, useful scope but
  young and not enough by itself for media-index semantics.
- `smpte-mxf 0.1.1`: partition/primer/RIP utilities, MIT, young and not a
  complete fit for QGS source-media modeling.
- `mxf 0.1.2`: older MXF reader/writer crate, MIT, limited maintenance signal.

## Dependency Decision

No external MXF runtime dependency was added. Step 7 implements only the small,
bounded QGS-owned subset required for controlled OP1a H.264 fixtures. This
keeps licensing simple and avoids adopting an immature or too-broad abstraction
before the QGS media model stabilizes.

FFmpeg/libx264 is used only as fixture-generation tooling.

## Implemented MXF Subset

Implemented in `qgs-mxf`:

- file-size bound
- KLV triplet scan
- BER length decode
- header/body/footer partition recognition
- safe skipping of unknown KLV triplets
- H.264 generic-container essence KLV recognition for the generated fixtures
- QGS media source, track, timecode, descriptor, and media-index models
- bounded access-unit extraction by index entry
- nearest prior random-access lookup

The parser is fixture-driven and does not claim complete SMPTE MXF coverage.

## KLV And BER Model

KLV keys are 16-byte ULs. BER lengths are decoded with explicit width checks,
overflow checks, maximum KLV value limits, and EOF validation before slicing.

M2 limits:

- maximum MXF file size: 64 MiB
- maximum non-filler KLV value: 8 MiB
- maximum KLV triplets: 8192
- maximum video index entries: 4096
- maximum compressed access unit: 4 MiB
- maximum track/model batch count: 64

## Partition Handling

`qgs-mxf` recognizes header, body, and footer partition pack keys and records
their file offsets. It does not yet interpret every partition-pack field or
RIP/body SID relationship.

Generated fixtures contain three recognized partitions.

## Metadata And Track Model

The QGS model contains:

- `MediaSource`
- `MxfTrack`
- `TrackKind`
- `TrackId`
- `VideoEssenceDescriptor`
- `AudioEssenceDescriptor`
- `Timecode`
- `MediaIndex`

Track identity is explicit and separate from vector position. No Qnc Project,
Clip, or Timeline concepts were added.

## Rational And Edit-Rate Model

Edit rates are represented as exact rationals with non-zero denominator
validation. The generated fixtures report a QGS edit rate of `25/1`.

## Timecode Model

The Step 7 timecode model preserves:

- start frame
- edit rate
- drop-frame flag

The generated fixtures start at frame `0` with edit rate `25/1` and
drop-frame `false`.

## Essence Descriptor Model

For video, `qgs-mxf` records:

- codec
- coded/display dimensions
- bit depth
- chroma subsampling
- optional essence-container UL
- optional compression UL

For Step 7, the video descriptor is derived from the first H.264 access unit
and records whether the parsed essence is 8-bit 4:2:0 or 10-bit 4:2:2. Future
work should parse more descriptor metadata directly from structural metadata
sets.

## Audio Discovery Model

The model includes an audio descriptor shape for future audio tracks:

- essence identity
- channel count
- sample rate
- bit depth

The generated fixtures are video-only, so no audio tracks are reported.

## Media Index Design

Each `VideoIndexEntry` stores:

- QGS track id
- edit-unit position
- presentation position
- KLV file offset
- payload byte range
- random-access classification
- index source

Offsets and lengths are checked before extraction. The current generated index
is QGS-derived by scanning H.264 essence KLVs.

## MXF-Provided Vs QGS-Derived Index

The Step 7 implementation distinguishes index source. The generated fixtures
are reported as `QgsDerived`; QGS does not claim the entries came from a parsed
MXF Index Table Segment.

## Random-Access API

Implemented:

- `MediaIndex::video_entry(n)`
- `MediaIndex::nearest_random_access_before(n)`
- `MediaSource::extract_video_access_unit(...)`

For Long-GOP fixtures, the middle-frame proof starts at the nearest IDR/random
access entry and feeds `qgs-codec-h264` statefully through the target access
unit. It does not pretend every P/B picture is independently decodable.

## Fixture Generation

Generated fixtures:

- `tests/fixtures/mxf/h264-8bit-420-long-gop-128x72.mxf`
- `tests/fixtures/mxf/h264-10bit-422-long-gop-128x72.mxf`

Generation commands are recorded in:

- `tests/fixtures/mxf/generate-mxf-fixtures.sh`
- `tests/fixtures/mxf/README.md`

FFmpeg rejected the initial 6 fps MXF attempt as an unsupported MXF frame rate,
so the committed fixtures use broadcast-valid 25 fps.

## Generated Fixture Properties

8-bit fixture:

- container: MXF OP1a
- codec: H.264 Main
- bit depth: 8-bit
- chroma: 4:2:0
- dimensions: 128 x 72
- edit rate: 25/1
- edit units: 12

Professional fixture:

- container: MXF OP1a
- codec: H.264 High 4:2:2
- bit depth: 10-bit
- chroma: 4:2:2
- dimensions: 128 x 72
- edit rate: 25/1
- edit units: 12

## H.264 Essence Extraction Result

`qgs-test --mxf-inspect tests/fixtures/mxf/h264-8bit-420-long-gop-128x72.mxf`
reported 12 video index entries, one random-access point, and extracted the
middle edit unit by starting at edit unit 0.

## qgs-codec-h264 Integration Result

The extracted 8-bit fixture target classified as:

- profile: Main
- dimensions: 128 x 72
- bit depth: 8
- chroma: 4:2:0
- picture kind: B

The extracted professional fixture target classified as:

- profile: High422
- dimensions: 128 x 72
- bit depth: 10
- chroma: 4:2:2
- picture kind: B

`qgs-codec-h264` remains MXF-unaware.

## Professional 10-bit / 4:2:2 MXF Fixture

The installed FFmpeg/libx264 build successfully generated a valid H.264 High
4:2:2 10-bit MXF fixture. QGS parses, indexes, extracts, and classifies it. It
does not attempt hardware decode.

## Malformed-Input Tests

Added coverage for:

- truncated KLV key
- malformed BER length
- BER length overflow width
- KLV value beyond EOF
- invalid partition offset
- excessive batch count
- excessive index count
- invalid track reference
- invalid essence offset
- truncated index table model
- unknown UL skipped where legal
- zero rational denominator
- invalid timecode metadata
- oversized access unit

## Test Results

`cargo test --workspace` passed.

Total: 182 tests passed.

- `qgs-codec-h264`: 6 passed
- `qgs-core`: 26 passed
- `qgs-linux`: 4 passed
- `qgs-mxf`: 17 passed
- `qgs-protocol`: 118 passed
- `qgs-vaapi`: 9 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Unsafe Inventory

No unsafe Rust was added for Step 7. `qgs-mxf` uses
`#![forbid(unsafe_code)]`. The QGS-owned unsafe block count remains 38 from
the existing audited qgs-vulkan interop boundaries and diagnostics.

## Commit And Push Verification

This report is included in the M2 Step 7 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after commit and push.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Limitations

Step 7 does not implement:

- complete MXF structural metadata parsing
- primer/local-tag resolution for all metadata sets
- MXF-provided Index Table Segment interpretation
- persistent index caching
- daemon file-open protocol
- audio decode or playback
- software video decode
- product/workflow classification
- MPEG-2/XDCAM decode
- HEVC
- video encode
- VA -> Vulkan zero-copy resume
- Qnc timeline behavior

## Recommendation For M2 Step 8

Extend MXF support toward real professional sample coverage: parse more
structural metadata and descriptor fields directly, add real audio track
discovery from MXF metadata, and introduce an explicit fixture matrix for legal
camera-originated samples before adding product/workflow classification.

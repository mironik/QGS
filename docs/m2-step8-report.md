# QGS M2 Step 8 Report

## Summary

M2 Step 8 moves `qgs-mxf` beyond fixture-only essence scanning by adding real
Primer Pack resolution, bounded local metadata set parsing, a small MXF
metadata graph, metadata-derived descriptors, audio discovery, MXF Index Table
Segment parsing, and RIP parsing.

No daemon file-opening protocol was added. MXF inspection remains a local
`qgs-test` tool/library path. The frozen Haswell VA -> Vulkan path was not
modified.

## Real Sony FX6 Compatibility Update

An external read-only Sony FX6 compatibility sample was inspected after the
synthetic Step 8 foundation. The MXF copy's SHA-256 matched the expected
compatibility-corpus hash. The media and XML sidecar were not copied into the
repository and are not committed.

Privacy handling: this report uses an anonymized sample description and omits
the local path, original clip basename, serial number, full private UMIDs, and
private production timestamps.

### Initial Parser Result On Real FX6 MXF

Before the final compatibility patch, the existing Step 8 parser successfully
reported:

- MXF OP1a operational pattern
- 106 video edit units at 50/1
- two package objects
- 64 metadata sets
- video descriptor: H.264, 10-bit, 4:2:2, stored 1920 x 1088, display
  1920 x 1080
- four independent mono PCM audio descriptors at 48 kHz, 24-bit
- one MXF-provided Index Table Segment with 106 entries
- random access lookup for the middle edit unit
- H.264 classification as High 4:2:2, 10-bit, 4:2:2, 1920 x 1080

The initial gap was that the SMPTE 436M ANC/data track was not represented in
the public `MxfTrack` model, and the inspector did not yet summarize GOP shape
or compare the Sony XML sidecar.

### Synthetic Vs Camera Differences Found

Compared with the synthetic OP1a fixtures, the camera sample adds:

- much larger header metadata and index areas
- header partition with BodySID `2` and IndexSID `1`
- footer partition without an index SID
- Material Package with 8 track references and Source Package with 7 track
  references
- stored video height padded to 1088 with display height 1080
- 4 independent 24-bit mono PCM audio tracks
- SMPTE 436M ANC/data track
- Index Table Segment with 7 delta entries and 106 index entries
- random-access positions at edit units `0, 24, 48, 72, 96`

### Primer Pack Result

The FX6 Primer Pack parsed successfully with 120 local-tag mappings. QGS uses
the primer to resolve local tags to property ULs; no FX6-specific local tag
numbers were hardcoded.

### Metadata Graph Result

The bounded graph parser resolved the package/reference shape needed for this
sample without duplicate `InstanceUID` or missing-reference errors. Strong
reference batches remained within configured limits.

### Operational Pattern

The operational pattern parsed from partition metadata is OP1a:

`060e2b34040101010d01020101010900`

### Partition And RIP Structure

The sample has:

- header partition at offset 0, BodySID `2`, IndexSID `1`
- footer partition near EOF, BodySID `0`, IndexSID `0`
- RIP with two partition entries

QGS preserves partition offsets and SIDs without assuming the synthetic fixture
layout.

### Video Descriptor Result

MXF CDCI metadata reports:

- codec family: H.264/AVC by essence coding metadata
- stored dimensions: 1920 x 1088
- display dimensions: 1920 x 1080
- component depth: 10
- chroma: 4:2:2
- edit/sample rate: 50/1
- aspect ratio: 16/9
- progressive frame layout

The stored height padding is treated as container storage metadata, not as a
codec/display mismatch.

### H.264 SPS/PPS Cross-Check

Extracted access units classify through `qgs-codec-h264` as:

- H.264 High 4:2:2
- 10-bit
- 4:2:2
- 1920 x 1080
- progressive

The H.264 SPS agrees with MXF display dimensions, bit depth, and chroma.

### Actual GOP Structure

The inspected 106 access units contain:

- I pictures: 5
- P pictures: 31
- B pictures: 70
- MXF index random-access edit units: `0, 24, 48, 72, 96`

The first, middle, and final sampled edit units classified as I, B, and P
respectively. This proves Long-GOP structure without decoding frames.

### Audio Track Discovery

QGS discovers four independent audio tracks:

- track 3: mono PCM, 48 kHz, 24-bit
- track 4: mono PCM, 48 kHz, 24-bit
- track 5: mono PCM, 48 kHz, 24-bit
- track 6: mono PCM, 48 kHz, 24-bit

The tracks are not flattened into one implicit 4-channel stream.

### SMPTE 436M ANC/Data Track Discovery

QGS now represents a generic `TrackKind::Data` with a QGS-owned
`DataEssenceDescriptor`. The FX6 sample reports one data track with 50/1 sample
rate and a SMPTE 436M ANC-related essence identity. QGS does not interpret ANC
payloads yet.

### MXF Timecode Result

MXF Timecode Component metadata parses as:

- start frame: `2506142`
- rate: `50/1`
- drop-frame: `false`

### Sony XML Timecode Comparison

The Sony sidecar XML reports an LTC table with `tcFps=25` and `halfStep=true`.
That representation is compatible with 50p half-step timecode semantics, but it
is intentionally reported as a separate XML source rather than collapsed into
the MXF Timecode Component.

### Duration And Edit-Unit Result

MXF/index parsing reports 106 video edit units at 50/1, matching the sidecar
duration value of 106. This corresponds to 2.12 seconds without hardcoding the
reference observation.

### Index Table Segment Result

The FX6 Index Table Segment parsed successfully:

- IndexEditRate: 50/1
- IndexDuration: 106
- IndexSID: 1
- BodySID: 2
- DeltaEntryArray entries: 7
- IndexEntryArray entries: 106
- signed temporal/key-frame offsets preserved
- stream offsets preserved with checked arithmetic

### MXF-Provided Vs QGS-Derived Index Comparison

The FX6 file provides a usable MXF index, so QGS marks entries as
`MxfProvided`. QGS-derived KLV scanning still supplies and bounds compressed
essence byte ranges for extraction. The two agree sufficiently for the random
access proof; no malformed-index fallback was needed.

### Random-Access Proof

For target edit unit 53:

- nearest MXF random-access point: edit unit 48
- QGS extracted bounded H.264 access units from edit unit 48 through 53
- `qgs-codec-h264` parsed the stateful sequence
- target picture classified as B, High 4:2:2, 10-bit, 4:2:2

No hardware decode was attempted.

### Capability Result On Current Intel

The current Intel HD Graphics 4600/i965 capability query still reports H.264
Baseline/Main/High 8-bit 4:2:0 decode only. It does not advertise H.264 High
4:2:2 10-bit decode. Therefore this real FX6 stream is valid and parsed, but
current Intel hardware selection must reject it as unsupported decode
configuration.

### Synthetic Regression Tests Added

Added generic synthetic unit coverage for `TrackKind::Data` and
`DataEssenceDescriptor` so normal tests do not require the external FX6 media.

## Primer Pack Implementation

`qgs-mxf` now parses Primer Packs as counted 18-byte local-tag mappings:

- `local_tag: u16`
- `property UL: 16 bytes`

Local-set parsing resolves semantic properties through the Primer Pack instead
of treating local tag numbers as globally meaningful. Tests cover the same
semantic property using different local tags.

## Metadata Graph Model

The parser models bounded metadata sets with:

- set key UL
- optional `InstanceUID`
- resolved properties
- strong-reference batches for known reference properties

The graph validation detects duplicate `InstanceUID`s and missing strong
references for the supported graph subset. It avoids recursive traversal and
uses explicit count limits.

## Package, Track And Reference Handling

Implemented package handling covers Material Package and Source Package sets,
including track strong-reference batches. Track identity remains separate from
array position, essence stream identity, and future application timeline IDs.

The current FFmpeg OP1a fixtures report:

- one Material Package
- one Source Package
- two tracks for video-only fixtures
- three tracks for stereo audio fixture
- four tracks for two-mono-track fixture

## Video Descriptor Fields

`qgs-mxf` now parses CDCI descriptor metadata where present:

- stored width/height
- sampled width/height
- display width/height
- display offsets
- frame layout
- component depth
- horizontal and vertical subsampling
- aspect ratio
- sample/edit rate
- essence container UL
- compression UL

For the generated fixtures, MXF metadata reports stored height `80` and display
height `72`; H.264 SPS reports `128 x 72`. QGS treats this as legal storage
padding and cross-checks display semantics against codec semantics.

## Metadata Vs H.264 Validation

`qgs-mxf` preserves both metadata-derived descriptor values and H.264
SPS-derived classification. It emits structured diagnostics for disagreement
instead of silently overwriting one source.

The committed fixtures have no metadata/H.264 diagnostics after accounting for
stored-vs-display dimensions.

## Audio Discovery

Wave audio descriptors are parsed for:

- essence identity
- channel count
- sample rate
- quantization bits
- block alignment
- average bytes per second

No audio decode or playback was added.

## Multiple-Audio Handling

The two-mono-track fixture parses as two independent audio tracks:

- track 3: mono PCM, 48 kHz, 16-bit
- track 4: mono PCM, 48 kHz, 16-bit

Audio tracks are not flattened into one implicit stereo stream.

## Timecode Handling

Timecode Component parsing preserves:

- start frame
- rounded timecode base as a rational rate
- drop-frame flag

The audio fixtures generated with `-timecode 01:00:00:00` parse as start frame
`90000` at `25/1`, drop-frame `false`.

## Index Table Segment Support

Footer Index Table Segments are parsed through the Primer Pack/local-set path.
The implemented subset records:

- IndexEditRate
- IndexStartPosition
- IndexDuration
- EditUnitByteCount
- IndexSID
- BodySID
- SliceCount
- DeltaEntryArray
- IndexEntryArray

Counts and byte arithmetic are validated before allocation.

## Temporal And Key-Frame Offset Model

Index entries preserve signed temporal offsets and signed key-frame offsets.
Unit tests cover negative values explicitly. The random-access helper uses
MXF-provided key-frame information when a valid index is present.

## BodySID And IndexSID Handling

Partition packs record BodySID and IndexSID. The generated fixtures have:

- body partition: BodySID `1`
- footer partition: IndexSID `2`
- index segment: BodySID `1`, IndexSID `2`

QGS no longer assumes that the first body partition contains the desired
essence without checking SIDs.

## RIP Result

The generated FFmpeg fixtures include a Random Index Pack. `qgs-mxf` parses RIP
entries and reports the header, body, and footer partition byte offsets.

## Operational Pattern Handling

Operational Pattern UL is parsed from partition packs. The committed fixtures
are OP1a:

`060e2b34040101010d01020101010900`

Unsupported operational patterns are reported as diagnostics rather than being
silently treated as OP1a.

## Fixture Matrix

Committed synthetic fixtures:

- `h264-8bit-420-long-gop-128x72.mxf`: OP1a, H.264 Main, 8-bit 4:2:0,
  25/1, Long-GOP I/P/B, video only.
- `h264-10bit-422-long-gop-128x72.mxf`: OP1a, H.264 High 4:2:2, 10-bit
  4:2:2, 25/1, Long-GOP I/P/B, video only.
- `h264-8bit-420-long-gop-128x72-pcm-stereo.mxf`: OP1a, H.264 Main plus PCM
  signed 16-bit stereo at 48 kHz, non-zero timecode.
- `h264-8bit-420-long-gop-128x72-two-mono.mxf`: OP1a, H.264 Main plus two
  independent PCM signed 16-bit mono tracks at 48 kHz, non-zero timecode.

Generation commands are recorded in `tests/fixtures/mxf/README.md` and
`tests/fixtures/mxf/generate-mxf-fixtures.sh`.

## Generated Audio Fixture Result

`qgs-test --mxf-inspect` reported for the stereo fixture:

- one video track
- one audio track
- channels: `2`
- sample rate: `48000/1`
- bit depth: `16`
- timecode start frame: `90000`
- MXF-provided index: 12 entries

For the two-mono fixture it reported two distinct audio tracks, each mono,
48 kHz, 16-bit.

## Synthetic Professional 10-bit 4:2:2 Result

The professional fixture parses from MXF metadata as:

- H.264
- bit depth `10`
- chroma `4:2:2`
- display dimensions `128 x 72`

The extracted access unit classifies through `qgs-codec-h264` as High 4:2:2,
10-bit, 4:2:2. No hardware decode is attempted.

## MXF-Provided Vs Derived Index Result

Step 8 prefers the valid MXF Index Table Segment in the committed fixtures and
marks video entries as `MxfProvided`. The QGS-derived KLV scan remains the
fallback/cross-check path and still supplies the bounded essence byte ranges.

## Random-Access Result

For the middle edit unit in each 12-frame fixture, `qgs-test --mxf-inspect`
found the nearest prior random-access point at edit unit `0`, extracted access
units from that point, and fed them statefully to `qgs-codec-h264` for
classification.

## Malformed-Input Coverage

Unit tests cover:

- Primer Pack parsing
- alternate local tags resolving to the same UL
- strong-reference resolution
- missing reference
- duplicate InstanceUID
- bounded reference batch
- video descriptor fields
- metadata/H.264 consistency and mismatch diagnostics
- PCM audio descriptor
- multiple audio track model
- Timecode Component including non-zero start
- Index Table Segment entries
- signed temporal offset
- signed key-frame offset
- DeltaEntryArray and IndexEntryArray bounds
- BodySID/IndexSID association
- RIP parsing
- truncated KLV key
- malformed BER length
- BER length overflow
- KLV value beyond EOF
- invalid partition offset
- excessive batch count
- excessive index count
- invalid track reference
- invalid essence offset
- truncated index table
- unknown UL skipping
- zero rational denominator
- invalid timecode
- oversized access unit

## Test Results

`cargo test --workspace` passed.

Total: 198 tests passed.

- `qgs-codec-h264`: 6 passed
- `qgs-core`: 26 passed
- `qgs-linux`: 4 passed
- `qgs-mxf`: 31 passed
- `qgs-protocol`: 118 passed
- `qgs-vaapi`: 9 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 2 passed
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Unsafe Inventory Confirmation

No unsafe code was added. `qgs-mxf` remains `#![forbid(unsafe_code)]`.

The QGS-owned unsafe block count remains `38`, all in the previously audited
`qgs-vulkan` boundaries.

## Commit And Push Verification

This report is included in the M2 Step 8 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after commit and push.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Limitations

- The parser still implements a bounded professional subset, not the complete
  SMPTE MXF ecosystem.
- Metadata graph resolution is limited to the structural references required by
  current fixtures and near-term camera-source probing.
- MXF-provided indexes are parsed for the implemented fields only.
- qgs-mxf does not decode audio or video.
- qgs-mxf does not classify vendor/product workflow families.
- No daemon path-opening protocol was added.

## Recommendation For Camera-Originated Compatibility Testing

Begin a legally obtained camera-originated sample matrix next. Prioritize
technical coverage:

- H.264/AVC Long-GOP 10-bit 4:2:2 MXF
- H.264/AVC Intra 10-bit 4:2:2 MXF
- professional AVC Long-GOP/Intra MXF from more than one vendor
- later MPEG-2 4:2:2 MXF compatibility material

Record codec, profile, bit depth, chroma, edit rate, timecode, audio layout,
operational pattern, partitions, index behavior, and descriptor/SPS agreement.
Keep vendor/product labels outside the low-level QGS API.

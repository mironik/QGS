# QGS M2 Step 5 Report

## Summary

M2 Step 5 extends the H.264 path from the Step 3B IDR-only proof to a
stateful Long-GOP decode proof with IDR/I, P, and B pictures, DPB state,
short-term references, reference-list construction, decode-order/display-order
separation, and explicit flush/drain behavior.

The Haswell VA -> Vulkan zero-copy path remains frozen. No VA -> Vulkan work,
10-bit/4:2:2 work, XAVC, MPEG-2/XDCAM, HEVC, or encode work was started.

## Fixture Structure And Provenance

Fixture:

- `tests/fixtures/h264/long-gop-128x72-main.h264`

Generated from FFmpeg `testsrc2` synthetic video with libx264:

```text
testsrc2=size=128x72:rate=6
frames: 12
profile: Main
pixel format: yuv420p
x264: keyint=12:min-keyint=12:scenecut=0:bframes=2:ref=2:open-gop=0:repeat-headers=1
```

`ffprobe` reports:

```text
codec_name=h264
profile=Main
width=128
height=72
pix_fmt=yuv420p
nb_read_frames=12
presentation picture types: I B P B B P B B P B B P
```

FFmpeg is a development fixture-generation tool only and is not a QGS runtime
dependency.

## h264-reader Suitability

`h264-reader 0.9.0` exposes the syntax needed for this milestone:

- P and B slice families
- frame number
- POC fields
- SPS/PPS reference fields
- `num_ref_idx_active`
- reference picture list modifications
- decoded reference picture marking

QGS implements the DPB/reference policy itself in `qgs-codec-h264`; parser
types do not escape that crate.

## Supported H.264 Subset

Implemented for Step 5:

- Annex B
- progressive frames
- 8-bit
- 4:2:0
- IDR/I, P, and B pictures
- POC type 0 for the Long-GOP proof
- minimal POC type 2 support sufficient for the existing IDR fixture
- short-term references
- sliding-window reference marking
- short-term unused-for-reference MMCO
- short-term reference-list modifications

Unsupported features are rejected explicitly, including interlaced/field
pictures, MBAFF, long-term references, MVC reference modifications, unsupported
POC modes for Long-GOP, 10-bit, and 4:2:2.

## POC Implementation

The Long-GOP fixture uses POC type 0. QGS calculates top/bottom field order
counts from `pic_order_cnt_lsb`, previous POC MSB/LSB state, and the SPS
`log2_max_pic_order_cnt_lsb_minus4` value. IDR pictures reset the POC state.

## DPB Model

`qgs-codec-h264` tracks decoded pictures with:

- QGS-owned picture identity: `frame_num` + POC
- reference/non-reference status
- output-needed status
- short-term reference metadata
- DPB occupancy and output-pending statistics

The DPB emits display-ready picture identities separately from reference
release identities.

## Reference Marking

Implemented marking behavior:

- IDR reset
- sliding-window eviction
- adaptive short-term unused-for-reference MMCO
- all references unused MMCO

Long-term reference operations remain unsupported.

## Reference Lists

`qgs-codec-h264` constructs P and B reference lists from the DPB and applies
short-term list modifications. `qgs-vaapi` maps the resulting QGS picture
identities to VA surface IDs; it does not invent reference ordering.

## Decode Order Vs Display Order

The fixture proves decode order is not presentation order. Submissions in
decode order produced:

```text
AU 00: 0 outputs
AU 01: 0 outputs
AU 02..11: 1 output each
flush: 2 outputs
```

Total display outputs: 12.

## Surface Lifetime Model

Decoded surfaces are kept in `qgs-vaapi` through shared ownership while they are
needed by the H.264 DPB. A `VideoSurface` can be handed to the session resource
registry while the decoder still retains the same VA surface as a future
reference. Display lifetime and reference lifetime are deliberately separate.

## Decoder Protocol Changes

`DECODE_OUTPUT` now returns a bounded list of output surfaces instead of exactly
one output. A successful submit may return zero outputs.

New request:

- `FLUSH_DECODER`: request kind `1`, opcode `13`

`DESTROY_DECODER` moves to request opcode `14`.

`DECODE_OUTPUT` remains response opcode `13`.

## Compressed Data Transport

The compressed access-unit limit is now 4 MiB. The overall v0.1 payload limit
is 4 MiB plus a small framing margin. This is a temporary M2 bounded compressed
media transport. Raw decoded pixels still do not cross normal QGS IPC.

## Flush And Drain

`FLUSH_DECODER` drains output-ready pictures that remain pending because of
reordering. It is not a substitute for destroying the decoder. Destroying a
decoder releases decoder state after normal use or disconnect.

## Intel Hardware Result

Intel HD Graphics 4600 / i965 result:

- created H.264 Main decoder
- decoded 12-frame synthetic Long-GOP stream
- decoded IDR/I, P, and B pictures
- used VA reference surfaces
- maintained DPB state
- output all 12 frames in presentation order
- flush returned the final 2 delayed frames
- destroyed output `VideoSurface` resources successfully
- destroyed decoder successfully

## Per-Frame Validation

`qgs-vaapi` performed validation-only VA readback after each hardware decode.
Checksums from the Long-GOP run:

```text
0xebac1c3a
0xaa34a2b7
0x0c168ff3
0x7b8e85f6
0x0a3f0d85
0xec96907f
0x3ca3908f
0x2a78beb2
0x2ab83e25
0x6a12608b
0xcd2ffba6
0xafd90803
```

This validates hardware-decoded output for every frame. It is a validation-only
CPU readback path, not the final video processing architecture.

## Memory Pressure Observations

Observed during the Intel Long-GOP proof:

- maximum DPB occupancy: 5
- maximum simultaneously live VA surfaces retained by the decoder: 5
- maximum output-pending pictures: 3

## NVIDIA Unsupported Behavior

NVIDIA GTX 950M / nouveau still reports zero VA decode capabilities.
`CREATE_DECODER` for the H.264 Main Long-GOP configuration failed cleanly with
`UnsupportedDecodeConfiguration`. No software fallback was attempted.

## Test Results

`cargo test --workspace` passed.

Total: 158 tests passed.

- `qgs-codec-h264`: 4 passed
- `qgs-core`: 26 passed
- `qgs-linux`: 4 passed
- `qgs-protocol`: 114 passed
- `qgs-vaapi`: 8 passed
- `qgs-vulkan`: 2 passed
- `qgs-test`: 0 tests
- `qgsd`: 0 tests
- doctests: 0 tests

## Fmt And Clippy

`cargo fmt --all -- --check` passed.

`cargo clippy --workspace --all-targets -- -D warnings` passed.

## Unsafe Inventory

No new unsafe Rust was added for Step 5. The QGS-owned unsafe block count
remains 38 from the existing audited qgs-vulkan interop boundaries and
diagnostics. `qgs-codec-h264`, `qgs-vaapi`, `qgs-core`, `qgs-protocol`, `qgsd`,
and `qgs-test` remain `#![forbid(unsafe_code)]`.

## Cleanup And Disconnect

The hardware run left one IDR decoder and one decoded `VideoSurface` alive for
disconnect cleanup. `qgsd` logged:

```text
client disconnected; releasing 1 resource(s), 0 sync object(s), and 1 decoder(s) for session 1
qgs-core: releasing 1 resource(s) owned by session
qgs-core: releasing 1 decoder(s) owned by session
```

## Commit And Push Verification

This report is included in the M2 Step 5 commit. The final commit hash cannot
be embedded in this committed file without making the commit self-referential;
the exact hash is reported after commit and push.

After push, `main` and `origin/main` are expected to match. The exact
verification is reported after push.

## Limitations Before 10-bit/4:2:2

Remaining before XAVC-class work:

- no H.264 High 10 or High 4:2:2 decode
- no 10-bit surface path
- no 4:2:2 surface path
- no interlaced/MBAFF support
- no long-term reference support
- no XAVC/MXF/container semantics
- no VA -> Vulkan zero-copy resume

## Recommendation For M2 Step 6

Proceed to the next codec capability deliberately. The strongest next step is
to address H.264 10-bit/4:2:2 parser/model rejection and backend capability
handling before attempting XAVC-class decode on hardware that may not support
it. MPEG-2/XDCAM should remain a separate path with its own codec frontend.

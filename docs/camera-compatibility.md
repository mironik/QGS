# QGS Camera Compatibility Notes

This document records privacy-safe technical compatibility observations from
legally obtained external camera samples. The original media files and sidecar
metadata are not part of the QGS repository.

QGS records technical container, codec, essence, audio, timecode, and index
behavior here. Product/workflow classification remains future work and must not
be encoded into low-level QGS protocol types.

## Sony FX6 Sample 001

Status: external read-only compatibility corpus, not committed.

Hash verification:

- MXF SHA-256 matched the expected compatibility-corpus hash.

Container:

- MXF OP1a
- duration: 106 edit units
- edit rate: 50/1
- MXF-provided Index Table Segment present
- RIP present

Video:

- codec: H.264 / AVC
- profile: High 4:2:2
- bit depth: 10-bit
- chroma: 4:2:2
- display dimensions: 1920 x 1080
- stored dimensions: 1920 x 1088
- scan: progressive
- GOP: Long-GOP with I/P/B pictures

Audio:

- 4 independent mono PCM streams
- 48 kHz
- 24-bit

Data:

- SMPTE 436M ANC data track present
- QGS discovers the track and preserves descriptor identity
- ANC payload interpretation is not implemented

Sidecar XML technical comparison:

- camera class/model: Sony FX6
- video codec tag indicates AVC 50 Mbit/s class, 1920 x 1080, High 4:2:2
- frame rate metadata reports 50p
- audio metadata reports 4 LPCM24 channels
- color/acquisition metadata reports Rec.709-related values

QGS result:

- `qgs-mxf` parses structural metadata, descriptors, timecode, index, audio
  tracks, and data track.
- H.264 access units extracted from MXF classify through `qgs-codec-h264` as
  High 4:2:2, 10-bit, 4:2:2, 1920 x 1080.
- Metadata, H.264 SPS, and sidecar XML agree on the core technical video shape.
- Current Intel Haswell VA-API capability still does not support H.264 10-bit
  4:2:2 hardware decode. This is a capability mismatch, not malformed media.
- Step 9 software fallback decodes all 106 presentation frames through
  `rsmpeg`/libavcodec while preserving `yuv422p10le` 10-bit 4:2:2 software
  VideoSurface semantics.
- Random access to edit unit 53 starts from the MXF-derived random-access point
  at edit unit 48 and produces the same decoded-frame checksum as sequential
  decode.
- Step 10 uploads the software-decoded `yuv422p10le` VideoSurface planes to
  Vulkan storage buffers and runs a fixed GPU YCbCr -> RGBA validation shader.
  The proof passes on Intel HD Graphics 4600 and NVIDIA GTX 950M / NVK with no
  CPU pixel copy between the software VideoSurface and the GPU processing input
  beyond the explicit fallback upload.
- Step 10 validates first frame, sequential edit unit 53, random-access edit
  unit 53, and final frame. Sequential and random-access edit unit 53 produce
  the same GPU output checksum.

Privacy handling:

- The original media filename, local filesystem path, camera serial number,
  full UMIDs, and private production timestamps are intentionally omitted.

## Sony FX6 Sample 001 Proxy

Status: external read-only damaged-media diagnostic case, not committed.

Proxy container:

- MP4 / ISO BMFF family
- H.264/AVC proxy stream declares 106 video samples
- AVCDecoderConfigurationRecord uses profile 100, level 42, 4-byte NAL length
  fields, 1 SPS, and 2 PPS

Damage finding:

- QGS and ffprobe agree on the damaged packet byte range for video sample 97.
- The first AVC NAL length in that sample is zero.
- Strict QGS AVC normalization rejects the sample as malformed.
- Strict FFmpeg decoding with `-xerror` also fails.
- FFmpeg frame hash diagnostics produced only 96 decoded frames.

QGS result:

- QGS reports the proxy as damaged/unrecoverable for complete proxy playback.
- The original MXF remains valid and is unaffected by this proxy damage.
- The malformed camera packet is not copied into the repository; a synthetic
  zero-length-NAL regression covers the parser policy.

## Sony FX6 Sample 002 Original/Proxy Pair

Status: external read-only compatibility corpus, not committed.

Hash verification:

- Original MXF SHA-256:
  `52a82d3527717096892a78bfd62f62f864e891fc4321e09ad7ed0856a7b9f24e`
- Sony XML sidecar SHA-256:
  `985d1f373616801601b2da2b3c1458d09aac2e5a6dbf12ae374bd3ee16c904e6`
- Proxy MP4 SHA-256:
  `fa7b646f7dc84bee84744dbaee89924b7f94982405c2fc3ffea2936618f73007`

Original:

- container: MXF
- video: H.264 High 4:2:2, 10-bit, 4:2:2, 1920 x 1080, 50p
- duration: 106 edit units
- audio: professional PCM topology
- data: ANC/data track present where described by the MXF

Proxy:

- container: MP4 / ISO BMFF family
- major brand: XAVC
- video: H.264 High, 8-bit, 4:2:0, 1920 x 1080, 50p
- AVC sample representation: length-prefixed AVC with 4-byte NAL lengths
- samples/read frames/read packets: 106 / 106 / 106
- audio: AAC at 48 kHz, 2 channels
- data: additional MP4 data track present and preserved as a data track

Association evidence:

- Filename similarity is intentionally not used as proof.
- Sony sidecar metadata reports proxy/substream metadata.
- Sidecar duration matches the 106-frame source/proxy structure.
- Original edit rate and proxy presentation rate are both 50/1.
- QGS observes one proxy presentation sample for each original edit unit.
- Association confidence: strong metadata plus timing evidence.

QGS result:

- `qgs-mp4` parses the proxy container, tracks, AVC configuration, sample
  tables, timing, and sync/random-access samples.
- `qgs-codec-h264` classifies the proxy as H.264 High, 8-bit, 4:2:0,
  1920 x 1080.
- Proxy GOP structure contains I and P pictures with random-access positions
  at samples 0, 48, and 96.
- Intel HD Graphics 4600 / i965 advertises the required H.264 High 8-bit
  4:2:0 decode capability and decodes all 106 proxy presentation frames through
  qgs-vaapi.
- The proxy path exposed and fixed two generic H.264/VA issues: short-term
  `frame_num` wrap handling for PicNum/reference ordering, and DPB-released VA
  surface reuse.
- Selected proxy frames feed the reusable GPU processing path on Intel HD
  Graphics 4600 and NVIDIA GTX 950M / NVK. VA -> Vulkan zero-copy remains
  frozen and is not used.

Privacy handling:

- The original filenames, local filesystem paths, camera serial number, full
  UMIDs, and private production timestamps are intentionally omitted.

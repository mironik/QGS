# M2 Block V - Live Preview Video + Original Audio Output

Block V makes the existing QGS Broadcast Player live command produce practical
output while it runs, with V2 cleanup making the default path honest and quiet:

- diagnostic proxy-video preview artifacts when explicitly requested
- original-MXF-derived PipeWire desktop monitor audio when explicitly requested

It does not add QNC UI, IPC, export/render, a Wayland/Vulkan presenter, a
second player state machine, production audio routing, realtime certification,
or A/V sync certification.

## V2 Cleanup

The first Block V pass made `preview-files` the default and reported short
PipeWire chunks as submitted. That was useful diagnostically, but it did not
feel like a clean live player path.

V2 corrects that:

- default live output is now `--video-output none --audio-output none`
- `preview-files` is labeled diagnostic preview output, not player display
- compact ticks use `video=diagnostic-written` when preview files are enabled
- repeated VA/MESA/libva informational spam is reduced by quiet backend
  environment settings for explicit diagnostic preview-file runs
- `pipewire-desktop-monitor` is added as a practical original-MXF-derived
  desktop monitor mode
- legacy `pipewire-monitor` remains accepted but is reported as
  `pipewire-desktop-monitor`

The audio model is unchanged: original MXF audio is authoritative, proxy AAC is
not used, and the runtime keeps the four original mono lanes discrete. The
desktop monitor pair is a listening helper, not production routing.

## Baseline From Blocks T/U

The live command remains:

```bash
--qgs-broadcast-player-live <original-mxf> <proxy-mp4>
```

It still drives:

```text
QgsBroadcastPlayerAssembly
  -> QgsQncControlSurface
  -> QgsBroadcastPlayerOperationalRuntime
```

Block U stdin controls remain available:

- `pause`
- `play` / `resume`
- `seek <frame>`
- `status`
- `stop`
- `quit`
- `help`

## Video Output Mode

Implemented diagnostic mode:

```text
--video-output preview-files
```

This mode uses the existing proxy MP4 decode/GPU/readback path and writes real
proxy frame pixels to:

```text
target/qgs-live-preview/frame_000000.ppm
target/qgs-live-preview/frame_000001.ppm
...
target/qgs-live-preview/latest.ppm
target/qgs-live-preview/latest.json
```

`latest.json` reports public-safe live state:

- current frame
- media time
- original-audio sample range
- prepared window
- source mode: `ProxyPreview`
- video source: proxy MP4
- audio source: original MXF
- proxy AAC authoritative: false
- real display: no
- visual verified: false
- private path exposed: false

This is diagnostic visible preview artifact output, not player display and not
a real display presenter.

`--video-output preview-window` is reserved and currently reports unsupported.
QNC OS display direction remains Wayland + Vulkan, not X11.

## Audio Output Mode

Implemented modes:

```text
--audio-output pipewire-desktop-monitor
--audio-output pipewire-monitor
--audio-output pipewire-4mono
```

Both modes use original MXF PCM as the source. Proxy AAC is not used.

The live loop takes the current runtime audio sample range from the QNC control
surface snapshot, extracts the matching original MXF PCM range, converts only
at the PipeWire device boundary, and submits bounded f32 interleaved buffers.

`pipewire-desktop-monitor` submits an audible-length desktop monitor segment
from the current playhead using original mono track 4 then track 1 as the
monitor pair. This mirrors the earlier Mironik 2002 diagnostic finding that
track 4 / track 1 was the best listening pair for that range. It is explicitly
desktop monitoring, not broadcast routing.

`pipewire-4mono` submits tracks 1..4 as channels 1..4 at the device boundary.
For current device-boundary stability, each `pipewire-4mono` submission is
capped to at most five logical video frames of original-audio samples.

Source truth remains:

- original MXF audio is authoritative
- four discrete mono lanes remain addressable
- proxy AAC is non-authoritative

`pipewire-monitor` is retained as a legacy alias for
`pipewire-desktop-monitor`. It does not certify production routing, physical
channel mapping, or AudioDeviceVerified.

`pipewire-4mono` submits tracks 1..4 as channels 1..4 at the device boundary.
It still does not certify physical channel mapping or production routing.

## Command Options

New options:

| Option | Default | Meaning |
| --- | --- | --- |
| `--video-output none|preview-files|preview-window` | `none` | Preview output mode. `preview-files` is diagnostic; `preview-window` is not implemented yet. |
| `--audio-output none|pipewire-desktop-monitor|pipewire-monitor|pipewire-4mono` | `none` | Original-MXF-derived audio output mode. |
| `--output-dir <path>` | `target/qgs-live-preview` | Preview artifact directory. |
| `--preview-every <n>` | `100` | Emit diagnostic preview output every `n` logical frames. |

Existing options remain:

- `--start-frame`
- `--status-every`
- `--max-frames`
- `--seek-frame`
- `--view compact|detailed`
- `--pace logical|wall`
- `--no-interactive`

## Playhead Coupling

Video output uses the live frame from the runtime snapshot.

Audio output uses the live original-audio sample range from the runtime
snapshot.

Pause stops further live-loop output submissions. Resume continues from the
current runtime position. Seek changes the next output frame/sample range.

The implementation is intentionally not a realtime scheduler. It submits
bounded chunks for observability and usefulness.

## Example

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <original-mxf> <proxy-mp4> \
  --max-frames 500 --status-every 25 \
  --video-output none \
  --audio-output pipewire-desktop-monitor \
  --view compact
```

Compact output now includes output status:

```text
tick=000000 frame=0 t=00:00:00.000 audio=[0..960) window=[0..6) buffer=ready video=off audio=submitted-monitor
```

## Sample 002 Result

Command shape:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <sample-002-original-mxf> <sample-002-proxy-mp4> \
  --max-frames 80 --status-every 10 \
  --video-output preview-files \
  --audio-output pipewire-monitor \
  --view compact --no-interactive
```

Observed result:

- run result: completed
- frames processed: 80
- video output: diagnostic preview files
- preview artifact: `target/qgs-live-preview/latest.ppm`
- preview metadata: `target/qgs-live-preview/latest.json`
- audio output: `pipewire-monitor` legacy alias, now reported as
  `pipewire-desktop-monitor`
- audio submitted: yes
- audio submissions: 16
- audio bytes copied: 1228800
- private path exposed: no
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio production verified: no
- A/V sync verified: no
- real `FramePresented` claim: no

The observed `latest.json` carried public-safe state, including proxy MP4 video,
original MXF audio, `proxy_aac_authoritative=false`, `real_display=no`, and
`private_path_exposed=false`.

Detailed mode was also run with a short 10-frame bound. It preserved the richer
runtime sections and reported diagnostic video output and original-MXF-derived
audio output on live ticks.

## Mironik 2002 Result

Command shape:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <mironik-2002-original-mxf> <mironik-2002-proxy-mp4> \
  --max-frames 50 --status-every 25 --preview-every 25 \
  --video-output preview-files \
  --audio-output pipewire-monitor \
  --view compact --no-interactive
```

Observed result:

- source frames: 10194 at 50 fps
- run result: completed
- frames processed: 50
- video output: diagnostic preview files
- audio output: `pipewire-desktop-monitor`
- audio submitted: yes
- audio submissions: 2
- audio bytes copied: 153600
- private path exposed: no
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio production verified: no
- A/V sync verified: no
- real `FramePresented` claim: no

The Mironik V2 live-audio acceptance uses `--video-output none` and
`--audio-output pipewire-desktop-monitor` so the operator output stays clean
and focused on the practical original-MXF monitor path.

## Interactive Behavior

A wall-paced interactive run accepted:

- `status`
- `pause`
- `seek 50`
- `play`
- `quit`

`pause` stopped live progression at the current frame, `seek 50` updated the
prepared window and original-audio sample range, `play` resumed from frame 50,
and `quit` stopped and unloaded the source. The run ended with private path
exposure still reported as `no`.

## Non-Claims

Even when preview files and PipeWire submissions succeed:

- real display: `NotImplemented`
- QNC OS display target: Wayland + Vulkan
- X11: legacy/non-target only
- visual verified: no
- realtime verified: no
- production audio verified: no
- A/V sync verified: no
- real `FramePresented`: no
- AudioDeviceVerified: no
- channel certification: no
- production routing: no
- proxy AAC used: no

## Limitations

- Preview output is file-based, not a window.
- Preview-file output is diagnostic and not the default player output.
- Preview files are bounded by `--max-frames` and `--preview-every`.
- PipeWire audio submission is chunked and bounded, not continuous realtime
  playback.
- `pipewire-desktop-monitor` is a practical listening helper, not production
  routing.
- `pipewire-4mono` remains available for discrete-lane device-boundary
  submission, but physical output mapping is not certified.
- Pause/resume/seek affect future submissions, but no certified A/V sync policy
  exists yet.
- Physical speaker channel mapping is not certified.

## Block W Follow-Up

Block W resets the live audio coverage cursor on accepted seek and stop, reports
`covered` instead of `playing` when a range was already submitted, and labels a
successful desktop-monitor chunk `submitted-monitor`. The verification matrix
now records Blocks T, U, and V below real display, production audio, and
realtime.

## Recommended Next Block

Recommended next milestone:

```text
M2 Block X - Real PreparedInput Fixture
```

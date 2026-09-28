# M2 Step 20F — QGS Broadcast Runtime Preroll Plan

This milestone adds a backend-neutral preroll planning layer to the QGS Broadcast Runtime state-machine skeleton from Step 20E.

It is still not real playback. QGS does not open an audio device, does not present to a display, does not run a real-time playout loop, and does not import QNC crates. The purpose is to model the conditions a future QNC application can require before a runtime session is allowed to enter `Ready`.

## Source Modes

The runtime contract is not proxy-video-only.

QGS now models two video source modes:

- `ProxyPreview`: video source is the camera-generated proxy MP4, while audio remains the authoritative original MXF audio. This is the current responsive preview/edit path for modest journalist hardware.
- `OriginalMedia`: video source is the original MXF and audio remains the authoritative original MXF audio. This mode is required for powerful laptops, workstations, finishing, and original-quality workflows.

Proxy MP4 AAC is not authoritative audio in either mode. It remains diagnostic/fallback-only media.

Step 20F acceptance primarily validates `ProxyPreview` using the Sony FX6 sample 002 proxy path. `OriginalMedia` is represented as a valid contract mode, but realtime original MXF video support is not claimed in this milestone. The preroll layer reports that mode as capability-missing when the original-video runtime path is not integrated.

## Preroll Model

`BroadcastPrerollConfig` describes the minimum prepared working set:

- video source mode
- selected video frames required before `Ready`
- original-audio ranges required before `Ready`
- finite video queue capacity
- finite audio queue capacity
- finite presentation queue capacity

`BroadcastPrerollPlan` describes the planned media work:

- video source mode
- selected video frames planned
- original-audio ranges planned
- intentional profile skips planned
- covered duration
- finite queue limits
- whether the source exists
- whether the runtime backend currently supports that source mode

`BroadcastPrerollStatus` reports whether preroll is ready, how many video/audio units are prepared, what is missing, whether queue limits are valid, and a clear not-ready reason.

The default Step 20F proof configuration uses:

- required video frames: 3
- required audio ranges: 3
- max video queue: 6
- max audio queue: 8
- max presentation queue: 3

These are bounded proof values, not final product policy constants.

## State-Machine Integration

`Prepare` now has a preroll-aware path. The state machine enters:

`Idle -> Preparing -> Ready`

only when media facts are valid and the preroll status is ready.

If preroll is not ready, the runtime remains in `Preparing`; `Play` remains invalid until `Ready` is reached. This preserves the broadcast-runtime rule that `Play` must not discover media or build the first usable working set.

Intentional `journalist-50i-preview` source-frame skips are profile behavior. They are not counted as missing preroll frames and are not lateness drops.

## Sony FX6 Sample Result

For Sony FX6 sample 002 in `ProxyPreview` mode:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- preview profile: `journalist-50i-preview`
- selected preview frames planned: 53
- intentional source-frame skips planned: 53
- original-audio ranges planned: 53
- prepared video frames: 3
- prepared audio ranges: 3
- finite queues: yes
- `Ready` reached only after preroll ready: yes
- `Play` from `Ready`: succeeds
- `Play` before `Ready`: rejected

The original media mode probe reports the original MXF video source as present in the contract, but the current Step 20F runtime does not claim realtime original-video support.

## Limitations

Not implemented in this milestone:

- real decoded video payload preroll queue
- real PCM payload playout queue
- audio device output
- display output
- real-time scheduling
- QNC UI integration
- export/render
- resampling or drift correction
- full original MXF video realtime playback acceptance

## Next Steps

The next runtime steps are to connect this contract to real bounded decoded payload queues, add an audio-device/master-clock strategy, and define the application-facing session/transport API that QNC OS can consume without pulling QNC UI policy into QGS.

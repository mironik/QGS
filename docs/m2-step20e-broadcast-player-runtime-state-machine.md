# M2 Step 20E - QGS Broadcast Player Runtime State Machine Skeleton

Step 20E adds the first backend-neutral QGS Broadcast Player Runtime state machine skeleton. It builds on the Step 20D QGS Broadcast Player Runtime contract and remains independent of any existing QNC Broadcast Player implementation.

This is not a UI player, not a real playback loop, and not audio/video device output. It models runtime state, commands, event sequencing, and accounting that future QNC applications can drive through a backend contract.

## Relationship To Step 20D

Step 20D defined the backend-neutral Broadcast Player Runtime contract facts:

- original MXF audio is authoritative
- proxy MP4 video is the preview/edit-performance source
- proxy MP4 AAC is diagnostic/fallback only and is not used
- `journalist-50i-preview` is a preview profile, not full interlaced rendering
- QGS exposes sample-clock-aware media facts and frame-to-sample range mapping

Step 20E adds the runtime state skeleton over those facts.

## State Model

The state machine models:

- `Idle`
- `Preparing`
- `Ready`
- `Playing`
- `Paused`
- `Draining`
- `Completed`
- `Failed`

The runtime starts in `Idle` after session creation. Preparation validates the already-proven media facts and moves the runtime to `Ready`. Playback commands then move through `Playing`, optional `Paused`, `Draining`, and `Completed`.

`Failed` is terminal for this skeleton. Preparing again after failure requires a new runtime session.

## Command Model

The small command surface is:

- `Prepare`
- `Play`
- `Pause`
- `Seek`
- `Stop`
- `Drain`

The implementation validates transitions. Examples:

- `Play` from `Idle` is rejected
- `Pause` from `Idle` is rejected
- `Seek` after `Completed` is rejected
- preparation with incomplete audio coverage fails the session

`Stop` from `Ready`, `Playing`, or `Paused` completes the skeleton session. This is conservative and avoids inventing a UI transport reset model in QGS.

## Event Model

Events include:

- `SessionCreated`
- `PreparingStarted`
- `ContractPrepared`
- `Prepared`
- `PlaybackStarted`
- `PlaybackPaused`
- `SeekCompleted`
- `FrameAccounted`
- `AudioRangeAccounted`
- `SimulatedPresentationDecision`
- `IntentionalProfileSkip`
- `DrainingStarted`
- `Completed`
- `Failed`
- `LatenessDrop`

`LatenessDrop` exists as a contract event type but is not emitted in the happy-path Step 20E acceptance run.

## Prepare Semantics

Preparation validates and binds:

- audio source: original MXF
- video source: proxy MP4
- proxy AAC: not used
- preview profile: `journalist-50i-preview`
- finite queue limits
- original audio timeline and PCM blocks
- proxy video timeline
- selected preview frames
- selected frame audio ranges are complete
- selected frames lie inside the original audio range
- audio block stream has no gaps or overlaps

No device output is opened during prepare.

## Play Semantics

`Play` does not rediscover media and does not perform real presentation. In the Step 20E proof it accounts a simulated playback sequence using prepared facts:

- one `FrameAccounted` event per selected preview frame
- one `AudioRangeAccounted` event per selected preview frame
- one `SimulatedPresentationDecision` event per selected preview frame
- one `IntentionalProfileSkip` event per profile-skipped source frame

Intentional skips are preview profile behavior and are not lateness drops.

## Sony FX6 Sample 002 Acceptance

Command:

```sh
cargo run -q -p qgs-test -- --broadcast-runtime-state-machine <original-mxf> <proxy-mp4>
```

Observed privacy-safe result:

```text
Audio source: original MXF
Video source: proxy MP4
Proxy AAC: not used
Preview profile: journalist-50i-preview
State sequence: Idle -> Preparing -> Ready -> Playing -> Paused -> Playing -> Draining -> Completed
Selected frames: 53
Intentional profile skips: 53
Audio ranges accounted: 53
Selected frames accounted: 53
Lateness drops: 0
Lateness drop events: 0
Final state: Completed
Happy path completed: yes
```

Invalid transition checks passed:

- `Play` from `Idle` rejected
- `Pause` from `Idle` rejected
- `Seek` after `Completed` rejected

## Limitations

Not implemented:

- real audio device
- real display output
- real-time scheduling
- export/render
- UI integration
- sync correction
- QNC integration
- speaker output
- Vulkan swapchain presentation
- waveform UI
- audio editing
- resampling or drift correction

No QNC crates are imported into QGS.

## Next Steps

Recommended next milestones:

- QNC application-facing session and transport command boundary
- runtime event stream shape for clients
- real audio output/device-clock boundary
- real display/presentation boundary
- sync policy over the original-audio sample clock
- seek/reset semantics beyond the skeleton contract

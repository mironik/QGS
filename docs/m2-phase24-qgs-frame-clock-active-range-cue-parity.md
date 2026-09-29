# M2 Phase 24 - QGS FrameClock / ActiveRange / Cue Parity

Phase 24 adds the timing core that Phase 23 intentionally left out. It is a
transport timing parity milestone, not a playback milestone.

The implementation follows the Step 22A replacement audit: QGS needs rational
frame timing, active range validation, cue/seek timing facts, latest-due-frame
accounting, and bounded due-frame drains before later prepared-buffer and tick
loop work can be honest.

## Relationship To Step 22A

Step 22A identified QNC `FrameClock` and `TransportEngine` timing behavior as a
gap in QGS. The relevant responsibilities are:

- exact rational frame timing
- active range boundaries
- cue/seek validation
- latest due frame calculation
- bounded due-frame draining
- forward, reverse, still, and simple rate behavior
- frame-to-original-audio sample mapping

Phase 24 implements those timing facts in QGS. It does not implement the QNC
continuous tick loop, playout buffer, presenter, audio output, or client
protocol.

## Relationship To Phase 22

Phase 22 created the QNC-compatible descriptor and `QgsInputPlan`. Phase 24 uses
the same source facts:

- public `qnc://` source identity
- selected video timebase
- selected video duration frames
- authoritative original MXF audio sample rate
- proxy AAC remains diagnostic-only
- original MXF mono audio remains authoritative

No second input model is introduced.

## Relationship To Phase 23

Phase 23 added source load/preload/set-active, active range, cue, prepared
anchor, and `play_ready`. Phase 24 provides the timing layer behind those facts:

- active range validation now uses `QgsActiveRangeTiming`
- cue validation maps through the timing layer
- cue facts report original-audio sample boundaries
- Play remains unchanged and does not consume a frame clock

## Runtime Types

Phase 24 adds these backend-neutral timing types in `qgs-media-runtime`:

- `QgsFrameClock`
- `QgsFrameClockMode`
- `QgsFrameClockRate`
- `QgsDueFrameDrain`
- `QgsActiveRangeTiming`
- `QgsCueValidation`
- `QgsFrameAudioSampleRange`

The existing `RationalRate` remains the frame-rate representation. Supported
tested rates include:

- 25/1
- 50/1
- 30000/1001
- 60000/1001

## Rational Timing Policy

Frame timing is rational. QGS does not treat fractional rates as integer FPS.
For example, 30000/1001 is not treated as exactly 30 fps, and 60000/1001 is not
treated as exactly 60 fps.

Frame-to-audio sample boundaries use the authoritative original-audio sample
rate and floor each rational boundary. Consecutive frame ranges are built from
adjacent boundaries, so they do not overlap. Fractional rates may produce
alternating sample counts, which is expected.

## Active Range Convention

Active ranges are half-open:

```text
[start_frame, end_frame)
```

Validation requires:

- `start_frame < end_frame`
- `end_frame <= source_duration_frames`
- cue frame satisfies `start_frame <= cue < end_frame`
- cue at `end_frame` is rejected
- latest due frame is bounded by `end_frame - 1`
- reverse drain is bounded by `start_frame`

## Cue / Seek Facts

Phase 24 cue handling is timing-only. It validates source/range facts and maps
the target frame to the original-audio sample boundary. It does not decode,
prepare payloads, fill queues, touch devices, or claim playback readiness on
its own.

## QGS-Test Command

Timing transcript command:

```bash
cargo run -q -p qgs-test -- \
  --qgs-frame-clock-parity <original-mxf> <proxy-mp4>
```

The command:

1. builds the Phase 22 descriptor
2. builds the Phase 22 `QgsInputPlan`
3. constructs the Phase 24 timing model
4. validates active range `[0, 1000 ms)`
5. validates cue frame 0
6. rejects cue at `end_frame`
7. reports frame/audio sample ranges
8. reports latest-due-frame examples
9. reports bounded forward, still, reverse, and simple double-rate behavior

## Acceptance Results

For Sony FX6 sample 002 / Mironik 1560:

- source frame rate: 50/1
- audio sample rate: 48000 Hz
- active range: `[0..50)` frames
- sample range: `[0..48000)` original-audio samples
- cue frame 0 valid: yes
- cue at `end_frame` rejected: yes
- frame 0 sample range: `[0..960)`
- frame 1 sample range: `[960..1920)`
- latest due at 0 ms: frame 0
- latest due at 60 ms: frame 3
- latest due at 1000 ms: frame 49
- forward drain at 60 ms: frames 0, 1, 2, 3
- still drain does not advance
- reverse drain starts from frame 49

For Mironik 2002:

- source frame rate: 50/1
- audio sample rate: 48000 Hz
- active range: `[0..50)` frames
- sample range: `[0..48000)` original-audio samples
- cue frame 0 valid: yes
- cue at `end_frame` rejected: yes
- frame 0 sample range: `[0..960)`
- frame 1 sample range: `[960..1920)`
- latest due and drain behavior matches sample 002 for the first 1000 ms range

## Non-Claims

Phase 24 does not implement or claim:

- realtime scheduler
- continuous tick loop
- playout buffer
- video presenter
- audio device output
- A/V sync
- QNC client protocol
- UI/export
- real playback

## Phase 25 Direction

The next phase should build on these timing facts without bypassing Phase 22 or
Phase 23:

- bounded prepared/playout buffer ownership
- source unload/close semantics
- tick/preparation loop
- latest-due integration with prepared slots
- QNC-compatible command/event envelope

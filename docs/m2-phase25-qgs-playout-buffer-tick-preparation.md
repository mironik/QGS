# M2 Phase 25 - QGS Prepared/Playout Buffer And Tick Preparation

Phase 25 adds bounded prepared/playout buffer ownership and a deterministic
tick-preparation step for the QGS transport stack.

This is not realtime playback. It does not submit frames to a presenter, send
audio to a device, run an A/V sync policy, or expose a QNC client protocol.

## Relationship To Step 22A

Step 22A identified two remaining transport gaps after input and basic
transport parity:

- a bounded prepared/playout buffer around the carrier frame
- a tick/preparation loop that advances prepared state without doing work on
  `play()`

Phase 25 implements the first QGS version of those gaps. The buffer is
transport-owned prepared state. It is not device-presented state.

## Relationship To Phase 22

The buffer consumes the Phase 22 `QgsInputPlan` facts:

- public `qnc://` source identity
- selected picture source mode
- authoritative original MXF mono audio
- original audio sample rate
- discrete mono lane inventory
- proxy AAC diagnostic-only rule

No second input model is introduced.

## Relationship To Phase 23

Phase 23 owns source load/preload/set-active, active range, cue, prepared anchor,
and `play_ready`. Phase 25 uses the active source revision and active range but
does not make `play()` fill the buffer. Buffer preparation happens only through
explicit tick-preparation calls.

The no-work-on-Play counters remain zero.

## Relationship To Phase 24

Phase 25 uses the Phase 24 timing layer for:

- active range boundaries
- latest due frame
- bounded due-frame drains
- frame-to-original-audio sample ranges

For 50 fps / 48 kHz, the prepared audio ranges are:

- frame 0: `[0..960)`
- frame 1: `[960..1920)`
- frame 49: `[47040..48000)`

## Buffer Model

The buffer records prepared frame slots:

- frame number
- source revision
- original-audio sample range
- preserved original mono audio lanes
- video payload reference status
- frame/audio/video preparation status

Statuses are:

- `NotPrepared`
- `Preparing`
- `Prepared`
- `Accounted`
- `Discarded`

`Prepared` means the transport has a bounded prepared record. It does not mean
submitted, displayed, audible, realtime, or verified.

## Buffer Limits

Default Phase 25 limits:

- backward keep frames: 2
- forward prepare frames: 5
- max prepared frames: 8

The tick step discards old frames before preparing the next forward window so
the state remains bounded.

## Tick Preparation

Each deterministic tick:

1. validates the carrier frame inside the active range
2. drains due frame numbers using `QgsFrameClock`
3. discards frames older than the backward window
4. prepares future frames up to the forward window
5. stores original-audio sample ranges for each prepared frame
6. marks video payload as a source reference, not a presenter submission
7. emits a deterministic event transcript

Events include:

- `TickPreparationStarted`
- `DueFramesDrained`
- `FramePrepared`
- `AudioRangePrepared`
- `VideoPayloadMarkedReady`
- `OldFrameDiscarded`
- `PreparedWindowAdvanced`
- `TickPreparationCompleted`

Events do not include `FramePresented`, `AudioDeviceVerified`, or
`RealtimeVerified`.

## QGS-Test Command

```bash
cargo run -q -p qgs-test -- \
  --qgs-playout-buffer-tick <original-mxf> <proxy-mp4>
```

The command builds the Phase 22 descriptor, creates the Phase 23 transport
source/range/cue/anchor, verifies `play_ready`, creates the Phase 25 bounded
buffer, and runs ticks at carrier frames 0, 3, and 6.

## Acceptance Results

For Sony FX6 sample 002 / Mironik 1560:

- active range: `[0..50)` frames
- sample range: `[0..48000)`
- buffer limits: backward 2, forward 5, max 8
- carrier frames: 0, 3, 6
- tick 1 prepares frames 0-5
- tick 2 advances the prepared window and reaches the 8-frame bound
- tick 3 discards old frames 0-3 and prepares the next bounded window
- original mono audio lanes preserved: 4
- proxy AAC used: no
- no-work-on-Play counters remain zero
- frame presented: no
- device output: no
- realtime playback: no

For Mironik 2002:

- active range: `[0..50)` frames
- sample range: `[0..48000)`
- same bounded tick behavior over the first 1000 ms range
- original mono audio lanes preserved: 4
- proxy AAC used: no
- no realtime/device/presentation claim

## Non-Claims

Phase 25 does not implement or claim:

- realtime scheduler
- continuous realtime loop
- real device submission
- video presenter
- audio output policy
- A/V sync
- QNC client protocol
- UI/export
- `FramePresented`
- `AudioDeviceVerified`
- `RealtimeVerified`

## Phase 26 Direction

Next work should keep the same ownership boundaries and add:

- source unload/close semantics
- prepared buffer integration with future tick owners
- QNC-compatible command/event envelope
- later real presenter/audio output adapters without changing prepared-state
  semantics

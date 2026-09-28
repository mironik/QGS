# M2 Phase 22 - QNC Input Compatibility Layer

Phase 22 implements the first QGS backend input layer for replacing the input
side of the existing QNC Broadcast Engine / Broadcast Player stack.

This phase is not a transport engine, realtime player, A/V sync system, video
presenter, or audio-device milestone. It packages the media/runtime facts QGS
has already proven into a QNC-compatible prepared descriptor and a QGS
`InputPlan` equivalent.

## Relationship To Step 22A

Step 22A identified the replacement gap between QNC `PreparedInput` /
`InputPlan` and current QGS runtime proofs. Phase 22 addresses the input side of
that gap:

- QNC-compatible prepared descriptor
- QGS `InputPlan` equivalent
- URI/resolver-compatible public identity
- original/proxy media binding
- selected picture representation
- authoritative original mono audio inventory
- source duration/timebase
- validation parity for the current Sony FX6 corpus

Transport engine parity remains future work.

## QNC Concepts Mirrored

The QGS descriptor mirrors the important QNC `qnc-player-input` concepts:

- selected picture representation may be original or proxy
- authoritative audio representation remains original
- proxy and original timing must be compatible when proxy picture is selected
- original audio channels remain the selected audio inventory
- public source identity is URI-like and resolver-compatible
- private local paths are transport bindings only

The QGS implementation does not import QNC crates. It defines backend-neutral
types in `qgs-media-runtime` so QNC applications can later adapt through a
stable contract.

## Runtime Types

Phase 22 adds these core descriptor types:

- `QgsPreparedInputDescriptor`
- `QgsPreparedMediaBinding`
- `QgsPlaybackRepresentation`
- `QgsPreparedStreamLayout`
- `QgsPreparedAudioLayout`
- `QgsPreparedAudioChannel`
- `QgsPreparedSourceIdentity`
- `QgsOriginalProxyAssociationStatus`

It also adds these input-plan types:

- `QgsInputPlan`
- `QgsInputPlanSourceMode`
- `QgsInputPlanVideoSource`
- `QgsInputPlanAudioSource`
- `QgsInputPlanQueueRequirements`
- `QgsInputPlanCapabilityRequirements`

## URI And Private Binding Rule

The descriptor exposes public URI-like identities such as:

```text
qnc://local/media/original/<clip>
qnc://local/media/proxy/<clip>
qnc://local/db/project_workspace/<clip>
qnc://local/source/<clip>
```

The local file paths passed to `qgs-test` are only private bindings used for
acceptance. They are reported as present or missing, but they are not the public
media identity.

## Original/Proxy Binding

The descriptor records:

- original media URI
- proxy media URI
- private original path binding present yes/no
- private proxy path binding present yes/no
- original/proxy association status

For `ProxyPreview`, timing compatibility requires matching original/proxy frame
rate and duration frame count. Original duration is derived from the
authoritative original PCM audio sample range, and the original video timebase is
read from MXF track metadata when available.

## Proxy Video And Original Mono Audio

The preserved rule is:

- proxy MP4 may be selected for picture/preview performance
- original MXF audio remains authoritative
- proxy AAC is diagnostic-only
- original MXF mono tracks remain discrete lanes

The current Sony FX6 media model exposes four original mono lanes. The command
prints the actual MXF track IDs; for the tested files the source lanes are
reported as MXF track IDs 3, 4, 5, and 6:

```text
lane 1 -> MXF track id 3 channel 0
lane 2 -> MXF track id 4 channel 0
lane 3 -> MXF track id 5 channel 0
lane 4 -> MXF track id 6 channel 0
```

No stereo collapse is performed.

## Validation Model

Descriptor validation checks:

- contract version is present
- public identities use `qnc://` style URIs
- original private binding is present
- proxy binding is present when proxy picture is selected
- selected picture representation is valid
- original/proxy timing is compatible for proxy picture
- authoritative audio is original
- project audio sample rate matches original audio sample rate
- original audio channel inventory exists
- mono lane order is stable
- proxy AAC is not authoritative
- source duration and timebase are usable

Input-plan validation checks:

- descriptor validation succeeds
- queue requirements are finite and internally consistent
- video source URI and audio source URI are public identities
- audio source remains original
- discrete mono lane requirement is preserved
- proxy AAC remains diagnostic-only
- frame/sample mapping uses the original audio sample rate

## QGS-Test Commands

Descriptor report:

```bash
cargo run -q -p qgs-test -- \
  --qnc-prepared-input-descriptor <original-mxf> <proxy-mp4>
```

Input plan report:

```bash
cargo run -q -p qgs-test -- \
  --qgs-input-plan <original-mxf> <proxy-mp4>
```

Both commands use original MXF audio as authoritative and do not use proxy AAC.

## Tested Media Results

For Sony FX6 sample 002 / Mironik 1560:

- selected picture representation: `Proxy`
- authoritative audio representation: `Original`
- original/proxy association: `TimingCompatible`
- audio channel count: 4
- audio sample rate: 48000 Hz
- audio bit depth: 24 bit
- source duration: 2.120 s
- video timebase: 50/1 fps
- source duration frames: 106
- proxy duration frames: 106
- 1000 ms maps to 48000 samples
- descriptor validation: ok
- input plan validation: ok

For Mironik 2002:

- selected picture representation: `Proxy`
- authoritative audio representation: `Original`
- original/proxy association: `TimingCompatible`
- audio channel count: 4
- audio sample rate: 48000 Hz
- audio bit depth: 24 bit
- source duration: 203.880 s
- video timebase: 50/1 fps
- source duration frames: 10194
- proxy duration frames: 10194
- 1000 ms maps to 48000 samples
- descriptor validation: ok
- input plan validation: ok

Mironik 2002 is inspected through the streaming MXF audio index path; the full
large MXF is not loaded into memory for this descriptor path.

## What Remains For Phase 23

Phase 23 should start transport parity:

- source load/preload/set-active
- active range
- cue/seek
- prepared anchor
- play-ready enforcement
- no open/decode/fill/preroll on Play
- rational frame clock parity
- playout/prepared buffer ownership
- event envelope suitable for QNC timeline/client projection

## Non-Claims

Phase 22 does not implement:

- transport engine parity
- realtime playback
- A/V sync
- video presenter/display output
- audio-device output policy
- QNC UI integration
- QNC DB reader/writer
- export/render
- proxy AAC as audio truth

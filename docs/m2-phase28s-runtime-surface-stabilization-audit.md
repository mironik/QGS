# M2 Phase 28S - Runtime Surface Stabilization Audit

Phase 28S audits the QGS runtime surface created across Phases 22-28 before
adding command queues, IPC, video presenter work, or additional device/output
boundaries.

This phase is stabilization only. It does not add runtime features, queue
limits, IPC, QNC UI integration, video presentation, audio output, realtime
playback, A/V sync, export, or render behavior.

## Phases Audited

- Phase 22: QNC input compatibility layer
- Phase 23: TransportEngine parity skeleton
- Phase 24: FrameClock / ActiveRange / Cue parity
- Phase 25: Prepared/Playout buffer and tick preparation
- Phase 26: Source lifecycle and runtime event envelope
- Phase 27: QNC-compatible command/event/passive-view projection
- Phase 28: Session command execution boundary

The audit also rechecked the Step 22A replacement audit and the Step 20Q
verification matrix.

## Checklist Results

### QGS/QNC Ownership Boundary

Status: OK, with one wording correction made in qgs-test output.

QGS remains the backend owner for media/runtime/device readiness, source
lifecycle, timing facts, prepared state, and future backend clock policy. QNC
remains the DB, UI, workflow, forms, story/newsroom context, and orchestration
layer.

QGS does not import QNC crates. The QGS descriptors are QNC-compatible shapes,
not QNC-dependent types.

Correction made:

- `--broadcast-player-runtime-contract` output no longer says clock ownership is
  future QNC application/runtime policy. It now says:
  `Clock policy: future QGS Broadcast Player Runtime/device backend policy; QNC applications observe and control it through session and transport commands.`

### Input Identity

Status: OK.

Public runtime identity uses `qnc://` URI-like values. Private filesystem paths
remain qgs-test/private transport bindings. Phase 26-28 command, event, and
passive-view surfaces explicitly check that private paths are not exposed.

### Media Source Policy

Status: OK.

The current model remains:

- proxy MP4 video may be selected for picture/preview performance
- original MXF mono audio is authoritative
- proxy AAC is diagnostic-only
- four mono source lanes remain discrete
- stereo monitor helpers remain diagnostic only and are not runtime truth

No stereo collapse is introduced by Phases 22-28.

### Transport Policy

Status: OK.

`play_ready` requires valid active source state, active range, cue, and prepared
anchor. Play before Ready is rejected. Play does not open sources, decode, fill
queues, preroll, or prepare anchors. The no-work-on-Play counters remain
zero-valued and covered by tests.

### Timing Policy

Status: OK.

Rational frame rates are preserved. Active ranges are half-open. Cue at
`end_frame` is rejected. Frame-to-original-audio sample mapping uses the
authoritative original audio sample rate and deterministic rational boundaries.

### Prepared / Presented / Evidence Distinction

Status: OK.

The surface keeps these distinctions separate:

- `Prepared` does not mean `SubmittedToDevice`
- `SubmittedToDevice` does not mean `Presented`
- `Presented` does not mean `Verified`
- `FramePresented` requires presenter evidence
- `AudioDeviceVerified` requires real audio-device verification
- `RealtimeVerified` requires measured realtime acceptance with margin

The Step 20Q matrix remains conservative about test presenter, test audio sink,
PipeWire drain/submission, manual listening, and discrete 4-mono diagnostics.

### Event / Command Projection

Status: OK.

The Phase 26 internal runtime event envelope remains distinct from future QNC
IPC. Phase 27 projection is not IPC. Phase 28 command execution is in-process
only. No process launcher or QNC UI claim is made.

### Documentation Consistency

Status: OK after the clock-policy output wording correction.

The audited docs avoid cutter-only framing, avoid stereo-as-runtime-truth, keep
raw paths out of public identity, and keep realtime/playback/device claims below
the evidence actually produced.

## QGS-Test Command Inventory

Phase 22-28 command surfaces checked:

| Command | Purpose | Claim Boundary |
| --- | --- | --- |
| `--qnc-prepared-input-descriptor` | Build QNC-compatible prepared descriptor from private test bindings. | Descriptor only; no transport/player behavior. |
| `--qgs-input-plan` | Convert descriptor into QGS input plan. | Plan only; no transport/player behavior. |
| `--qgs-transport-engine-parity` | Exercise load/preload/set-active/range/cue/anchor/play-ready. | Transport skeleton; no realtime or device output. |
| `--qgs-frame-clock-parity` | Exercise rational timing, active range, cue, latest due, drain behavior. | Timing facts only; no playback loop. |
| `--qgs-playout-buffer-tick` | Exercise bounded prepared buffer and tick preparation. | Prepared state only; no submit/present/verify. |
| `--qgs-runtime-lifecycle-events` | Exercise close/unload lifecycle and internal event envelope. | Internal envelope only; no IPC. |
| `--qgs-qnc-event-projection` | Project internal facts into QNC-compatible command/event/passive views. | Projection only; no IPC/UI. |
| `--qgs-session-command-boundary` | Apply QNC-shaped commands in-process with generation checks. | In-process executor only; no command queue/IPC. |

The command naming/output is consistent with the current scope. No new audit
command was added.

## Wording Corrections Made

One qgs-test output string was corrected:

- Old wording: `Clock owner: future QNC application/runtime policy, not QGS UI`
- New wording: `Clock policy: future QGS Broadcast Player Runtime/device backend policy; QNC applications observe and control it through session and transport commands.`

No behavior changes were made.

## Remaining Risks

- Phase 28 command execution has no bounded command queue yet.
- Duplicate command-id handling is not defined yet.
- Phase 28 is still in-process and not IPC.
- qgs-test still uses private filesystem paths as acceptance bindings, even
  though public identity remains `qnc://`.
- No real display presenter exists.
- No full production audio output policy exists.
- No realtime Broadcast Player scheduler exists.
- No A/V sync policy exists.

## Recommended Next Phase

M2 Phase 29 - Bounded Command Queue / Duplicate Command ID Policy.

That phase should add command queue limits, command-id semantics, duplicate
handling, and queue rejection behavior without implementing IPC, QNC UI,
realtime playback, video presenter work, or audio output policy.

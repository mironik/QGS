# M2 Step 22A - QNC Broadcast Engine Replacement Audit

Step 22A audits how QGS should replace the existing QNC Broadcast Engine /
Broadcast Player backend stack. This is an audit only. It does not implement new
runtime behavior, continue audio experiments, or reduce QGS to a cutter-only
contract.

QGS must become the backend media/runtime/device foundation for the future
QGS/QNC OS Broadcast Player. QNC remains the DB, UI, application workflow, and
module orchestration layer.

## QNC Files Read

The audit inspected the current GitHub `mironik/QNC` reference checkout:

- `AGENTS.md`
- `crates/qnc-player-input/src/lib.rs`
- `crates/qnc-broadcast-player/src/lib.rs`
- `crates/qnc-broadcast-player/src/engine_contract.rs`
- `crates/qnc-broadcast-player/src/transport_engine.rs`
- `crates/qnc-broadcast-player/src/frame_clock.rs`
- `crates/qnc-broadcast-engine/src/lib.rs`
- `crates/qnc-broadcast-engine/src/input.rs`
- `crates/qnc-broadcast-engine/src/output.rs`
- `crates/qnc-audio-output/src/lib.rs`
- `crates/qnc-video-output/src/lib.rs`
- `crates/qnc-player-client/src/lib.rs`
- `crates/qnc-player-launcher/src/lib.rs`
- `crates/qnc-player-timeline/src/lib.rs`
- `crates/qnc-monitor/src/lib.rs`
- `crates/qnc-media-records/src/lib.rs`
- `crates/qnc-transport-resolver/src/lib.rs`
- `crates/qnc-work-settings/src/lib.rs`
- `crates/qnc-filmstrip/src/lib.rs`
- `crates/qnc-wave/src/lib.rs`

The audit also inspected current QGS runtime docs and code:

- `docs/m2-step20d-qgs-broadcast-player-runtime-contract.md`
- `docs/m2-step20q-broadcast-player-runtime-verification-matrix.md`
- `docs/m2-step21n-discrete-4mono-pipewire-output-boundary.md`
- `crates/qgs-media-runtime/src/lib.rs`
- `crates/qgs-audio-pipewire/src/lib.rs`
- `tools/qgs-test/src/main.rs`

## Existing QNC Architecture Summary

QNC is DB-first. Project and workflow facts are read from public DB/transport
contracts, not from UI state. Public media identity is QNC URI/resolver based;
raw OS paths are private transport bindings, not public player identity.

`qnc-player-input` converts saved project DB/work-settings and finalized media
records into `PreparedInput`. It selects the picture representation according to
`PlaybackInput`, but audio always remains the original representation through
`audio_media()`. When proxy picture is selected, proxy and original video timing
must be compatible, and `selected.audio_channels = original.audio_channels`.

`qnc-broadcast-player` is the transport/player core. Its exported contract
defines source-open, video-decode, audio-output, frame-presenter, playout-output,
and scheduler adapter boundaries. `TransportEngine` owns load, preload,
set-active, active range, cue/seek, readiness, prepared anchor, bounded playout
buffer, tick/preparation, and event generation. Its `play()` path explicitly
does not open sources, decode, fill queues, or perform deferred preroll.

`FrameClock` owns rational frame scheduling. It handles exact frame intervals,
fractional rates such as 30000/1001 and 60000/1001, reverse/still/rate
semantics, latest-due-frame behavior, and bounded draining of due frames.

`qnc-broadcast-engine` composes the player core with concrete decode, GPU,
audio output, and video output adapters on a player-owner thread. `InputPlan`
validates `PreparedInput`, creates `SourceRuntime`, maps saved stream layout,
keeps original audio media distinct from selected picture media, computes sample
boundaries from saved timebase/sample rate, validates native/project audio
layout, preserves mono source lanes, and creates output configuration.

`qnc-audio-output` owns the prepared PCM-to-device edge. It has a channel map,
bounded queue, generation/start/pause/commit model, underrun/failure telemetry,
driver timing, and playback-position reporting. It does not own media decode,
DB, UI, or the playback clock.

`qnc-video-output` owns the public video device edge: output config, prepared
frame pool, output slots, GPU completion, and submission. It does not own media
decode, DB, clock, or workflow.

`qnc-player-client` is the public process client. It launches/prepares the
player asynchronously, exposes a small action surface, maintains a passive view,
and exchanges protocol events. It does not own decoder, clock, database, or app
identity.

`qnc-player-launcher` maps `PreparedInput` and source transport bindings into a
launch request. It preserves QNC URI identity while using private local/network
bindings for transport.

`qnc-player-timeline`, `qnc-monitor`, `qnc-filmstrip`, and `qnc-wave` are
observers/projections. Timeline derives passive projection from player protocol
events. Monitor paints already-confirmed frame descriptors. Filmstrip and Wave
plan from saved snapshots and artifacts; they do not scan, probe, own playback
state, or become workflow owners.

## Replacement Map

| QNC component | Current responsibility | QGS replacement responsibility | Status in QGS | Gap |
| --- | --- | --- | --- | --- |
| `qnc-player-input` | Read work settings and saved media record, validate `PreparedInput`, select picture representation, preserve original audio channels. | Provide a QNC-compatible prepared media/session descriptor, source role model, URI identity fields, and original/proxy timing validation. | Partial. QGS has source modes, original/proxy association, original audio timeline, and proxy/original video modes. | No `PreparedInput`-compatible descriptor, DB/work-settings reader boundary, QNC URI identity, or validation parity yet. |
| `qnc-broadcast-player::engine_contract` | Adapter traits for source open, video decode, audio output, frame presenter, playout output, scheduler, and frame-atomic AV handoff. | Define backend-neutral QGS adapter contracts for media source open, payload preparation, audio device boundary, video presenter boundary, scheduler/clock policy, and event evidence. | Partial. QGS has runtime events, prepared slots, payload bindings, device-boundary statuses, test presenter/sink evidence, and PipeWire device boundary. | No single integrated adapter trait surface matching source/decode/audio/video/scheduler responsibilities. |
| `qnc-broadcast-player::TransportEngine` | Source load/preload/set-active, active range, cue, play-ready, prepared anchor, playout buffer, tick/preparation, event generation, no open/decode/preroll on Play. | Own QGS Broadcast Player Runtime transport state and command/event sequencing over prepared slots and device boundaries. | Early skeleton. State machine, preroll plan, prepared slots, event surface, and simulated loop exist. | Missing source load/preload/set-active, active range, cue/seek parity, prepared anchor, real tick/preparation loop, playout buffer, and no-work-on-Play enforcement in integrated runtime. |
| `qnc-broadcast-player::FrameClock` | Exact rational frame clock, latest due frame, drain due frames, reverse/still/rate behavior. | Own QGS sample-clock-aware frame scheduling facts and frame/audio mapping without UI/export FPS leakage. | Partial. QGS maps frame time to original-audio sample ranges and has rational timing helpers. | No QGS `FrameClock` parity type for active transport, rate, reverse, still, latest-due, or drain-due behavior. |
| `qnc-broadcast-engine::InputPlan` | Convert `PreparedInput` to engine plan, validate geometry/color/audio layout, compute sample boundaries, preserve mono source lanes, choose output config. | Convert QNC-compatible descriptor to QGS runtime session plan: media access, source mode, decode path, PCM lane plan, video payload path, queue limits, and output/device capability needs. | Partial. QGS can build current sample-specific runtime facts and payload bindings. | No reusable `InputPlan` equivalent, project audio layout validation, source URI resolver integration, or broad saved-metadata validation. |
| `qnc-broadcast-engine::Runtime` | Compose media decode, GPU conversion, audio output, presenter, transport engine, owner-thread tick, telemetry, play/pause/stop/cue. | Compose QGS media backends, prepared payload slots, device boundaries, transport engine, and telemetry behind a Broadcast Player Runtime API. | Partial proof layers exist in `qgs-test`; not a composed runtime object. | No production runtime owner object, no player-owner thread model, no continuous tick loop, no integrated telemetry or command surface. |
| `qnc-audio-output` | Prepared PCM device queue, channel map, underrun behavior, telemetry, playback position, start/pause/commit. | Native audio device boundary for original MXF PCM with discrete mono lane preservation, queueing, timing evidence, and future clock policy. | Partial. `qgs-audio-pipewire` can configure streams, submit/drain buffers, and submit discrete 4-mono diagnostic output. | No production queue/generation model, no device clock ownership, no underrun/rearm policy, no certified channel mapping, no full `AudioDeviceVerified`. |
| `qnc-video-output` | Output config, prepared frame pools, slots, GPU completion, frame submission. | Video presenter/device boundary for QGS processed proxy/original GPU payloads with presentation evidence. | Partial. QGS has processed GPU frame tokens, test presenter evidence, and no real display claim. | No real display presenter, swapchain/Wayland/X11/DRM/KMS boundary, frame pool ownership, or visual verification. |
| `qnc-player-client` | Public command/event client, async prepare/launch, bounded command queue, passive view. | QNC-facing QGS Broadcast Player client/session API with commands and events but no UI ownership. | Partial. Backend-neutral events exist; qgs-test prints summaries. | No process/client protocol, no stable command queue, no generation handling, no QNC-facing view projection. |
| `qnc-player-launcher` | Build launch from `PreparedInput` and source transport binding; raw paths remain private. | Accept QNC URI/resolver-compatible launch/session descriptors and bind private transport resources internally. | Missing. QGS qgs-test commands still take raw local paths for acceptance. | Need URI/resolver compatibility and private path binding boundary. |
| `qnc-player-timeline` | Passive timeline projection from player events. | Emit stable QGS runtime events sufficient for QNC timeline projection. | Partial. Event surface includes frame/audio accounting and runtime state. | Missing QNC-compatible event envelope, active range, carrier position, readiness, and boundary events. |
| `qnc-monitor` | Passive monitor painting from already-confirmed frame descriptors. | Provide frame/presenter descriptors and evidence suitable for monitor modules without making UI part of QGS. | Partial. Test presenter evidence exists; real display descriptors do not. | Missing monitor-ready frame transport descriptor and real presenter/display evidence. |
| `qnc-filmstrip` | Passive plan from saved metadata and artifacts. | Preserve media snapshot/timing facts so QNC filmstrip can continue to plan from saved records or QGS-derived snapshots. | Not directly implemented. QGS has media inspection facts but no QNC snapshot writer. | Need QNC media snapshot compatibility; QGS must not become filmstrip workflow owner. |
| `qnc-wave` | Passive waveform plan from saved audio metadata/artifacts; mono lanes A1-A4. | Provide original PCM lane facts and future waveform input while preserving discrete mono source identity. | Partial. Original MXF PCM metadata, extraction, runtime blocks, and mono lane diagnostics exist. | No QNC wave artifact contract, no production waveform planner, no peak generation integration. |

## Non-Negotiable QGS Replacement Rules

- QNC remains the DB, UI, application workflow, and work-settings layer.
- QGS owns backend media/runtime/device readiness.
- QNC `PreparedInput` semantics must be preserved or deliberately superseded by
  a compatible descriptor.
- Proxy video selection is for picture/preview.
- Original MXF mono audio remains authoritative.
- Proxy AAC is diagnostic-only.
- Four mono channels remain discrete source lanes.
- Play must not open, decode, fill, or preroll.
- Prepared readiness must happen before Play.
- Frame clock and timebase facts must come from saved media/runtime facts, not
  UI/export FPS.
- Source identity must be QNC URI/resolver compatible; raw OS paths must not be
  public identity.
- QGS events must distinguish submitted, presented, drained, verified, and
  manually observed evidence.

## Current QGS Overlap

QGS already overlaps substantial media/backend pieces:

- original/proxy association and sample-specific timing checks
- proxy MP4 inspection and proxy H.264/VA/Vulkan fallback path
- original MXF audio metadata, LPCM extraction, PCM runtime blocks
- original MXF video payload binding for bounded selected frames
- `ProxyPreview` and `OriginalMedia` source modes
- Broadcast Player Runtime contract skeleton, state machine, preroll plan,
  prepared slots, event surface, payload binding, and simulation
- device-boundary contract with explicit `PayloadReady`,
  `DevicePayloadReady`, submitted, evidence, and verified distinctions
- PipeWire native stream creation, buffer submission, drain evidence,
  desktop listening helper, and discrete 4-mono diagnostic boundary
- verification matrix that keeps test/manual/native boundary evidence below
  full audio/display/realtime verification

These are backend facts and proofs. They are not yet a full replacement for the
QNC transport engine, player client, launcher, or runtime owner.

## Gaps In QGS

### QNC PreparedInput-compatible descriptor

QGS needs a descriptor that can consume or mirror QNC `PreparedInput`: contract
version, workspace DB URI, playback input, project audio, representation,
snapshot, stream layout, and original audio inventory. Current qgs-test paths
are useful acceptance tools but are not QNC public identity.

### InputPlan equivalent

QGS needs an `InputPlan` equivalent that binds the prepared descriptor to QGS
decode/extraction paths, sample boundaries, queue limits, audio lane layout,
video source mode, output/device capabilities, and runtime payload policy.

### Source load/preload/set-active

QGS currently creates demo/session facts directly. It does not yet model
preloaded source handles, active source replacement, source revision checks, or
source close/unload semantics.

### Active range

The current selected-frame demos are deterministic but not a general active
range contract. QGS needs frame/sample active ranges that can drive cue,
prebuffer, playback boundary, and timeline projection.

### Cue/seek

QGS has timing and bounded extraction helpers, but no transport-level cue/seek
contract matching QNC `cue_frame(frame, present)` and seek preparation rules.

### Play-ready/prepared anchor

QGS has preroll readiness and prepared slots. It still needs a strict prepared
anchor model: Play succeeds only when the anchor frame/range has already been
prepared and device boundaries are in the correct committed state.

### Frame clock rational timebase

QGS frame-to-sample mapping is rational-aware, but it does not yet expose a
transport `FrameClock` equivalent with due-frame draining, late-frame policy,
reverse/still/rate behavior, and audio-clock/device-clock integration.

### Playout buffer

QNC has a bounded `PlayoutBuffer` containing video and primed audio around the
carrier frame. QGS has prepared slots and payload bindings, but no continuous
transport-owned buffer with refill, discard, and submit semantics.

### Tick/preparation loop

QGS has simulated playback and qgs-test acceptance commands, not a production
bounded tick loop that services decode, GPU collection, audio device state,
output submission, readiness, and rebuffering.

### Runtime-to-device audio payload sequence

QGS has original-audio payload binding and PipeWire diagnostic submission.
Missing are production queue generations, commit/start/pause semantics,
underrun handling, playback-position timing, and policy for device format
conversion while preserving original PCM truth.

### Discrete 4-mono output boundary status

The latest QGS boundary has `Discrete4MonoOutputDrainCompleted`: original tracks
1-4 submitted as output channels 1-4 and drained through native PipeWire. This
is not channel certification, production routing, realtime playback, A/V sync,
or `AudioDeviceVerified`.

### Video presenter/display boundary

QGS has processed proxy/original GPU payload tokens and test presenter
evidence. It still lacks a real video presenter, display/swapchain boundary,
presentation evidence from a real backend, visual verification, and output pool
ownership equivalent to `qnc-video-output`.

### Client command/event surface

QGS events exist, but there is no QNC-facing command/event protocol, generation
model, bounded command queue, process boundary, or stable passive view like
`qnc-player-client`.

### Launcher/process boundary

QGS has no QNC launcher equivalent. It must accept resolver-compatible media
identity and private bindings without exposing raw filesystem paths as public
session identity.

### Monitor/timeline projection

QGS runtime events are not yet shaped into QNC `CarrierPositionChanged`,
`ExecutionRangeChanged`, `PlaybackReadinessChanged`, or monitor frame payload
descriptors. QNC timeline and monitor should remain passive observers.

### QNC URI identity compatibility

Current qgs-test media commands take filesystem paths. That is acceptable for
acceptance, but not for QNC replacement scope. Public runtime descriptors must
use QNC URI/resolver-compatible identities.

## Recommended Next Milestones

1. **M2 Step 22B - QNC PreparedInput Compatibility Descriptor**
   Define a QGS descriptor that can represent QNC `PreparedInput` semantics:
   original/proxy binding, source mode, original audio inventory, project audio,
   stream layout, workspace identity, and resolver-compatible media URI.

2. **M2 Step 22C - QGS InputPlan Equivalent**
   Convert the descriptor into a validated QGS runtime plan with sample
   boundaries, audio lane layout, video source path, payload policy, queue
   limits, and capability requirements.

3. **M2 Step 22D - QGS Transport Engine Parity Skeleton**
   Add source load/preload/set-active, unload, active range, play-ready,
   prepared anchor, and explicit no-open/decode/preroll-on-Play transition
   rules.

4. **M2 Step 22E - QGS FrameClock / ActiveRange / Cue Parity**
   Implement rational frame clock behavior, cue/seek semantics, latest-due and
   drain-due frame accounting, rate/reverse/still policy, and active-range
   boundary handling.

5. **M2 Step 22F - QGS Playout Buffer And Tick Preparation Loop**
   Build the bounded transport-owned buffer and preparation loop over existing
   payload bindings, without adding realtime claims.

6. **M2 Step 22G - QGS Player Client Command/Event Surface**
   Define the QNC-facing session/transport command surface, generation model,
   event envelope, passive view facts, and timeline projection inputs.

7. **M2 Step 22H - QGS Video Presenter Boundary**
   Start the real display/presenter boundary, preserving the evidence rules:
   submitted is not presented, and test evidence is not real display output.

8. **M2 Step 22I - QGS Runtime-to-QNC Timeline/Monitor Projection**
   Provide passive event/frame descriptors that QNC timeline, monitor,
   filmstrip, and wave modules can consume without owning backend timing.

## What Not To Do

- Do not build a separate cutter-only path that bypasses the QNC player model.
- Do not continue audio experiments before transport/input parity is scoped.
- Do not replace QNC DB/workflow ownership with QGS.
- Do not use proxy AAC as audio truth.
- Do not collapse mono channels to stereo.
- Do not treat desktop listening helpers as broadcast channel certification.
- Do not claim realtime/full playback until measured with margin.
- Do not fake display or audio-device verification.
- Do not expose raw local paths as public source identity.
- Do not make QNC UI own media timing, backend readiness, or device clock
  policy.

## Audit Conclusion

QGS has already proven many low-level backend facts that the old QNC stack did
not own natively: original MXF audio extraction, PCM block modeling, GPU payload
tokens, device-boundary evidence separation, and native PipeWire 4-mono
diagnostics. The largest missing piece is not another codec or audio experiment;
it is the replacement contract that turns those facts into a QNC-compatible
Broadcast Player backend: prepared input, input plan, transport engine, frame
clock, playout buffer, client/event protocol, and resolver-compatible identity.

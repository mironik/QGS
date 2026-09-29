# M2 Block N - QNC Broadcast Engine Reuse / Parity Audit

Block N audits how the current QGS Broadcast Player Runtime, after Block M,
maps to the existing QNC broadcast player / broadcast engine stack.

This is an audit block. It does not implement runtime features, device
backends, UI integration, real display output, production audio output, export,
or realtime/A-V sync certification.

## Purpose

The purpose is to decide what QGS should reuse, adapt, replace, or leave in
QNC while moving toward a real modular Broadcast Player backend.

The practical conclusion is:

- QGS has replaced a meaningful subset of backend runtime concepts.
- QNC still owns UI, DB, workflow, and public project/media identity.
- QNC contracts and behavior are useful reference material.
- QGS must not copy QNC backend implementation blindly.
- The next useful block should shape a QNC-compatible command/snapshot/event
  control surface, not jump directly to Wayland/Vulkan.

## Current QGS State After Block M

QGS currently has these relevant Broadcast Player pieces:

- `QgsBroadcastPlayerAssembly` as modular composition root.
- `QgsPreparedInputDescriptor` / `QgsInputPlan` equivalents for the accepted
  local acceptance path.
- `ProxyPreview` source mode: proxy MP4 video plus original MXF audio.
- `OriginalMedia` source mode model: original MXF video plus original MXF audio.
- Original MXF audio metadata, LPCM extraction, PCM runtime blocks, and
  prepared payload binding.
- Discrete original MXF mono lanes preserved as runtime truth.
- Proxy AAC marked non-authoritative.
- Transport/load/prepare/cue/play/pause/seek/stop/unload skeletons.
- Logical frame-to-original-audio sample mapping.
- Bounded prepared window/preroll facts and no-play-before-ready rules.
- Operational runtime snapshots, event/fault surfaces, and control session.
- Device backend selection policy with Wayland+Vulkan as QNC OS target and X11
  as legacy/non-target.
- Presenter/audio boundary evidence below real display/audio verification.
- Verification matrix that avoids upgrading diagnostic evidence to real output.

## QNC Source Inspected

No local QNC checkout was present at `/home/miro/Development/qnc` or
`/home/miro/Development/QNC`, so the public repository was inspected:

```text
https://github.com/mironik/QNC
temporary audit checkout: /tmp/qnc-audit-1790707209
```

Inspected QNC files/modules included:

- `AGENTS.md`
- `Cargo.toml`
- `crates/qnc-player-input/src/lib.rs`
- `crates/qnc-broadcast-player/src/lib.rs`
- `crates/qnc-broadcast-player/src/engine_contract.rs`
- `crates/qnc-broadcast-player/src/transport_engine.rs`
- `crates/qnc-broadcast-player/src/frame_clock.rs`
- `crates/qnc-broadcast-engine/src/lib.rs`
- `crates/qnc-broadcast-engine/src/input.rs`
- `crates/qnc-broadcast-engine/src/output.rs`
- `crates/qnc-audio-output/src/lib.rs`
- `crates/qnc-audio-output/src/channel_map.rs`
- `crates/qnc-audio-output/src/model.rs`
- `crates/qnc-video-output/src/lib.rs`
- `crates/qnc-player-contract/src/lib.rs`
- `crates/qnc-player-contract/src/event.rs`
- `crates/qnc-player-contract/src/interface/protocol/*.rs`
- `crates/qnc-player-contract/src/session.rs`
- `crates/qnc-player-client/src/lib.rs`
- `crates/qnc-player-launcher/src/lib.rs`
- `crates/qnc-player-timeline/src/lib.rs`
- `crates/qnc-monitor/src/lib.rs`
- `crates/qnc-filmstrip/src/lib.rs`
- `crates/qnc-wave/src/lib.rs`
- `crates/qnc-transport-resolver/src/lib.rs`
- `crates/qnc-media-records/src/lib.rs`
- `crates/qnc-work-settings/src/lib.rs`

## QNC Module Responsibility Table

| QNC module | Current role | Reuse category | QGS equivalent | Action |
| --- | --- | --- | --- | --- |
| `qnc-player-input` | Reads work settings and saved media records into `PreparedInput`; selects picture representation; keeps audio from original representation. | `ReuseContract` | `QgsPreparedInputDescriptor`, `QgsInputPlan` | Reuse descriptor shape and validation semantics; QGS should consume a compatible descriptor rather than QNC DB directly. |
| `qnc-player-contract` | Neutral frame-based command/event/source/session protocol. | `ReuseContract` | QGS command/event/passive view surfaces | Align QGS public surface with frame-based commands, source runtime, session generation, and evidence distinctions. |
| `qnc-broadcast-player::engine_contract` | Adapter traits for source open, video decode, audio output, frame presenter, scheduler, playout output. | `ReuseContract` | QGS device/presenter/audio/source boundaries | Adapt concepts into QGS backend-neutral traits later; do not import QNC crate directly. |
| `qnc-broadcast-player::TransportEngine` | Load/preload/set-active, active range, cue, play readiness, prepared anchor, bounded playout buffer, tick/preparation, event generation. | `ReuseBehavior` | QGS transport, operational runtime, prepared window | Continue matching behavior; QGS implementation should own backend version. |
| `qnc-broadcast-player::FrameClock` | Rational frame clock, latest-due/drain-due, rate/direction/still semantics. | `ReuseBehavior` | QGS frame clock parity and frame/sample mapping | Reuse behavior/tests conceptually; QGS needs stronger active runtime clock later. |
| `qnc-broadcast-engine::InputPlan` | Converts `PreparedInput` into validated media/decode/output plan; validates geometry, color, audio layout and channel map. | `AdaptLater` | `QgsInputPlan` | Adapt fields and validation; QGS must use native MXF/proxy facts and Linux device boundaries. |
| `qnc-broadcast-engine::Runtime` | Owner-thread composition of decode, GPU conversion, audio output, video output, transport, telemetry. | `ReplaceInQGS` | `QgsBroadcastPlayerAssembly` plus future runtime owner | QGS should replace backend implementation while preserving composition idea. |
| `qnc-audio-output` | Prepared PCM-to-device edge with channel map, bounded queue, generation, commit/start/pause, telemetry, driver timing. | `ReuseBehavior` | `qgs-audio-pipewire`, QGS audio device boundary | Reuse generation/queue/telemetry behavior; QGS must implement Linux/PipeWire production backend. |
| `qnc-audio-output::ChannelMap` | Device routing only; preserves source channels; no stereo fallback. | `ReuseContract` | QGS discrete mono lane model and PipeWire 4-mono boundary | Strongly reuse semantics: identity routing for 4 mono lanes, no stereo collapse. |
| `qnc-video-output` | Public video device edge with output config, prepared frame slots, GPU completion, submission. | `ReuseContract` | QGS presenter boundary, file/readback diagnostics, future Wayland/Vulkan presenter | Adapt output slot/evidence model; backend implementation should be QGS/Linux-native. |
| `qnc-player-client` | Public process client with generation, async prepare/launch, passive view, bounded command queue. | `ReuseContract` | QGS control session, future QNC-compatible control surface | Reuse session/view/generation ideas; process/IPC can remain future boundary. |
| `qnc-player-launcher` | Maps `PreparedInput` plus private transport bindings into launch requests. | `AdaptLater` | QGS private path binding boundary | Keep public URI/private binding separation; QGS needs compatible backend binding input. |
| `qnc-player-timeline` | Passive timeline projection from player events; does not own player state. | `KeepInQNC` | QGS event surface supplies facts | Keep in QNC; QGS must emit enough events. |
| `qnc-monitor` | Passive monitor painting from confirmed frame descriptors. | `KeepInQNC` | QGS presenter/monitor descriptors | Keep UI painting in QNC; QGS supplies presenter evidence/frame descriptors. |
| `qnc-filmstrip` | Passive filmstrip plan from stored metadata/artifacts. | `KeepInQNC` | QGS media snapshots may feed future records | Keep in QNC; QGS should not own filmstrip workflow. |
| `qnc-wave` | Passive waveform plan/peaks from stored audio metadata/artifacts, including A1-A4 lanes. | `KeepInQNC` | QGS original PCM lane facts and future waveform inputs | Keep in QNC; QGS can provide original lane payload/peak inputs later. |
| `qnc-transport-resolver` | Public QNC URI to private local/network endpoint resolution. | `ReuseContract` | QGS source identity/private binding separation | Reuse URI/resolver semantics; QGS should not expose raw paths publicly. |
| `qnc-media-records` | Persisted media snapshot contract and final/camera phases. | `KeepInQNC` | QGS inspection/extraction facts may populate compatible snapshots later | QNC remains DB owner; QGS should consume compatible snapshots/descriptors. |
| `qnc-work-settings` | Project work settings read from DB/transport; playback input and audio settings. | `KeepInQNC` | QGS consumes prepared descriptor results | QNC remains project settings owner. |
| UI/forms/app crates | Project, Ingest, Shell, app surfaces and workflow. | `KeepInQNC` | None | Do not move to QGS. |
| Old backend implementation details tied to QNC process/window stack | Concrete legacy runtime/output assumptions. | `DoNotReuse` | QGS backend modules | Avoid copying implementation; use contracts/behavior only. |

## QGS Replacement / Parity Table

| QGS module/block | Replaces/Matches QNC concept | Current evidence | Remaining gap |
| --- | --- | --- | --- |
| `QgsPreparedInputDescriptor` | `qnc-player-input::PreparedInput` shape | Public URI identity, selected picture, original audio, project audio, original/proxy timing. | Needs stable serialized QNC-compatible schema and DB/work-settings integration. |
| `QgsInputPlan` | `qnc-broadcast-engine::InputPlan` | Source mode, proxy/original timing, queue requirements, original audio lane plan. | Needs resolver binding, saved color/geometry validation parity, broader source layouts. |
| `QgsBroadcastPlayerAssembly` | Broadcast engine composition root | `AssemblySurfaceEvidence` after Block M. | Needs adapter trait surface and long-running owner/session lifecycle. |
| `QgsBroadcastPlayerCore` | Product control facade | `ControlSurfaceEvidence`. | Needs stable QNC-facing protocol shape and serialization. |
| `QgsBroadcastPlayerOperationalRuntime` | Runtime command legality/position/buffer/fault surface | `OperationalStateEvidence`, `RuntimeBehaviorEvidence`. | Not realtime; no continuous scheduler/device loop. |
| QGS transport/Phase 23-28 surfaces | QNC `TransportEngine` responsibilities | Load/preload/set-active/range/cue/prepared anchor/no-work-on-Play skeletons. | Needs integrated continuous playout buffer/refill and stronger prepared anchor parity. |
| QGS frame clock/timing | QNC `FrameClock` and frame/audio mapping | Rational timing and frame-to-original-audio sample mapping. | Needs active rate/reverse/still/latest-due runtime semantics if required. |
| Prepared window/preroll | QNC prebuffer/play-ready behavior | Bounded window and Play-before-ready rejection. | Needs production decode/GPU/audio queue integration. |
| Device backend selection | QNC output config/device policy separation | Selection policy: preview, diagnostic, headless, future appliance. | Real Wayland/Vulkan and production PipeWire backends not implemented. |
| Presenter boundary | QNC frame presenter/video output edge | Test/file/readback diagnostics and payload tokens. | Real display presenter and presentation evidence missing. |
| Audio PipeWire boundary | QNC audio output device edge | Native PipeWire buffer/drain, discrete 4-mono diagnostic evidence. | Production queue/generation/clock policy and `AudioDeviceVerified` missing. |
| Fault/recovery | QNC runtime error/event policy | Structured faults, recovery suggestions, rejected-command no-mutation. | Needs stable external codes and app-facing recovery semantics. |
| Event/snapshot projection | QNC protocol/passive views/timeline inputs | QNC-shaped events/views with private path filtering. | Needs stable serialized schema and compatibility tests against QNC protocol. |
| Verification matrix | QNC truth/evidence discipline | Evidence levels prevent overclaiming. | Needs future upgrades only when real backend evidence exists. |

## QNC Concepts QGS Can Use

| QNC concept | Can QGS use it? | How | Notes |
| --- | --- | --- | --- |
| `PreparedInput` fields | Yes | Reuse/adapt as serialized QGS input descriptor. | QNC remains DB/work-settings owner. |
| `PlaybackInput` / selected picture representation | Yes | Map to `ProxyPreview` / `OriginalMedia`. | Audio remains original in all modes. |
| `StreamLayout` audio channel inventory | Yes | Map to QGS original mono lane plan. | Preserve stream/channel identity. |
| `ProjectAudio` | Yes | Use for expected output/channel requirements. | QGS should validate, not own project DB. |
| `SourceRuntime` | Yes | Map public source runtime facts into QGS source handle. | No raw paths. |
| `BroadcastPlayerProtocolCommand` names | Yes | Shape QGS external control surface. | QGS command enum may be adapted, not imported. |
| `BroadcastEvent` / protocol events | Yes | Shape QGS events and QNC projection. | Keep `VideoFrameSubmitted` distinct from `FramePresented`. |
| Session generation/query model | Yes | Use for stale-command rejection and passive state. | QGS already has generation-checked command boundary; needs product surface. |
| Frame-based request/range model | Yes | Use for active ranges, cue, play requests. | Original audio remains sample-based internally. |
| Transport no-work-on-Play rule | Yes | Keep as behavioral invariant. | Already enforced in QGS skeleton; keep strengthening. |
| Prepared anchor / play readiness | Yes | Reuse behavior. | Needs stronger integrated backend readiness later. |
| `ChannelMap::identity` semantics | Yes | Preserve discrete mono output channels. | Do not use desktop L/R helpers as runtime truth. |
| Timeline/monitor passive projection | Yes | Emit enough public events/descriptors for QNC modules. | QNC keeps UI painting/projection modules. |
| Filmstrip/wave plan concepts | Partially | Use snapshot/artifact compatibility later. | QGS should not own these workflows. |
| Transport resolver URI model | Yes | Consume URI/private binding boundary. | Raw local paths remain private. |

## Reusable QNC Contracts

Use as contract/shape:

- `PreparedInput` / `ProjectAudio` / `StreamLayout`.
- `SourceRuntime`, `FrameRange`, `Timebase`, `TransportStatus`.
- `BroadcastPlayerProtocolCommand`.
- `BroadcastPlayerProtocolEvent`.
- `SessionRequest`, `SessionReply`, generation validation.
- Monitor frame header / frame payload descriptor shape.
- QNC URI resolver model and private transport binding distinction.
- Audio `ChannelMap` as routing contract, not mixdown policy.

## Reusable QNC Behavior

Use as behavior:

- Play must not open media, decode, fill queues, or defer preroll.
- Ready requires source, active range, cue, prepared anchor/window, and output
  readiness as appropriate.
- Cue/seek must validate half-open active ranges.
- Carrier position events, not readiness alone, drive passive timeline playhead.
- Submitted frame is not physical presentation.
- `FramePresented` requires actual presentation evidence.
- Audio output submitted/drained is not acoustic verification.
- Source channels remain discrete; no implicit stereo downmix.
- Public source identity is URI/resolver based; private bindings stay private.

## QNC Code / Components Not To Reuse

Do not reuse as QGS runtime implementation:

- UI/forms/shell/application workflow code.
- DB ownership and project/work settings readers.
- Ingest/project/story/rundown workflow logic.
- Windows- or UI-host-specific output implementation details.
- Concrete old player process/client implementation as QGS internal design.
- Any raw path as public identity.
- Any stereo-collapse or desktop monitor helper as runtime audio truth.
- Old backend code that QGS is replacing with native Linux media/GPU/audio
  modules.

## What Remains In QNC

QNC remains owner of:

- UI/forms and user interaction.
- Project/story/rundown/newsroom workflow.
- Project DB, media records DB, work settings, and persistent snapshots.
- Application orchestration and module placement.
- Timeline, monitor, filmstrip, and wave UI/projection modules.
- Public QNC URI identity and project references.
- Transport resolver ownership as application/project identity policy.

QNC may issue commands and observe QGS events/snapshots. It should not own QGS
backend readiness, media timing facts, prepared payload rules, device evidence,
audio device internals, presenter internals, or original/proxy runtime mapping.

## What QGS Already Owns

QGS already owns or is now clearly expected to own:

- Backend media/runtime/device readiness.
- Source mode validation.
- Original/proxy runtime media mapping.
- Original MXF audio as authoritative runtime audio.
- Original MXF video payload path for `OriginalMedia` where supported.
- Proxy MP4 video payload path for `ProxyPreview`.
- Discrete original mono lane preservation.
- Prepared payload/window facts.
- Operational runtime command legality.
- Fault/recovery from backend runtime.
- Device backend selection policy.
- Presenter/audio boundary evidence.
- Frame/audio position facts while running.
- Verification truth matrix.

## Remaining QGS Gaps

Major gaps before QGS can replace the backend responsibilities of the current
QNC broadcast player engine:

- Stable serialized QNC-facing command/snapshot/event schema.
- In-process API or IPC boundary for QNC applications.
- Project media resolver / private transport binding integration.
- Full `PreparedInput` compatibility with saved QNC media records.
- Production owner/session lifecycle and long-running runtime control.
- Continuous tick/preparation loop with decode/GPU/audio service.
- Stronger prepared anchor/playout buffer parity.
- Real Wayland/Vulkan presenter and real display `FramePresented` evidence.
- Production PipeWire audio backend with queue/generation/device timing policy.
- A/V sync verification and device-clock policy.
- OriginalMedia control/session path beyond bounded payload proofs.
- Timeline/monitor integration contract tests.
- Formal playhead/media-time surface for operator/QNC UI.

## Risks

- Copying QNC backend implementation directly could recreate the monolith QGS is
  trying to avoid.
- Treating QNC UI/client code as backend truth would put media readiness and
  timing policy in the wrong layer.
- Treating PipeWire diagnostic evidence as production audio output would weaken
  the verification matrix.
- Treating screenshot/readback/test presenter evidence as real display output
  would weaken `FramePresented`.
- Reintroducing stereo assumptions would break the broadcast/news mono-lane
  audio model.
- Exposing raw paths in QNC-facing output would break public identity rules.

## Recommended Next Block

Recommended:

```text
M2 Block O - QNC-Compatible Broadcast Player Control Surface
```

Scope should be:

- shape stable command names, command IDs, generation checks, event names,
  snapshot fields, and passive view fields for future QNC apps
- map QGS assembly/runtime facts to QNC-compatible command/event/session
  concepts
- keep raw paths private
- preserve original MXF audio, proxy-preview video, original-media mode, and
  discrete mono lanes
- do not implement QNC UI, IPC, Wayland/Vulkan, production audio, realtime
  certification, or A/V sync certification yet

Do not recommend Wayland/Vulkan as the immediate next step. The control surface
needs to be stable enough first so real device backends can report into a
durable product API.

## Non-Claims

This audit does not claim:

- real display output
- real backend `FramePresented`
- visual verification
- realtime playback
- A/V sync
- production PipeWire audio output
- `AudioDeviceVerified`
- QNC UI integration
- export/render
- full backend replacement completion

It only records architecture reuse/parity evidence.

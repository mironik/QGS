# M2 Block S - QNC Bridge Prototype Plan

Block S plans the QNC-side bridge that will let QNC use QGS as the Broadcast
Player backend.

This is a planning block. It does not implement IPC, QNC UI integration, QNC
database readers in QGS, a `qnc-qgs-contract` crate, Wayland/Vulkan, X11,
DRM/KMS, production PipeWire audio, realtime playback, A/V sync, export/render,
or a monolithic player.

## Purpose

QNC remains the application:

- UI, forms, DB, workflow, project/story/rundown state.
- Media records, work settings, public media identity, and operator intent.
- Passive monitor, timeline, wave, and filmstrip views.

QGS remains the backend engine:

- Broadcast Player runtime behavior.
- Source validation and original/proxy mapping.
- Original MXF audio authority and discrete mono lane preservation.
- Prepared payload readiness, transport legality, preroll/prepared windows.
- Device boundary facts, events, snapshots, faults, and verification truth.

The future bridge should sit in QNC. It should map QNC-owned prepared input and
operator intent into the QGS control surface, then project QGS snapshots/events
back into QNC passive views. It must not become a second player engine.

## Current QNC Player Flow

The QNC audit checkout inspected for this block was:

```text
/tmp/qnc-audit-1790707209
```

No local checkout was present at `/home/miro/Development/qnc` or
`/home/miro/Development/QNC`.

Current QNC flow:

```text
QNC app/form
  -> work settings + media records
  -> qnc-player-input PreparedInput
  -> qnc-player-launcher private transport binding
  -> qnc-player-client generation-checked client
  -> current player runner / broadcast player backend
  -> qnc-player-contract EventEnvelope
  -> passive QNC views
```

| Question | Current QNC answer |
| --- | --- |
| Which layer builds `PreparedInput`? | `qnc-player-input::InputReader` reads `qnc-work-settings` and `qnc-media-records`, chooses the picture representation, and builds `PreparedInput`. Audio inventory remains from the original representation. |
| Which layer launches/controls the existing player? | `qnc-player-launcher::prepare_launch` combines `PreparedInput`, executable path, and private source binding. `qnc-player-client::Player` owns async prepare/load, generation, a bounded command queue, and connection polling. |
| Which crate owns command/session/client behavior? | `qnc-player-contract` owns command/event/session envelopes. `qnc-player-client` owns client generation and active connection behavior. |
| Which modules consume player events/snapshots? | `qnc-player-client::View`, `qnc-player-timeline`, `qnc-monitor`, and form adapters consume projected player facts. `qnc-wave` and `qnc-filmstrip` consume stored media metadata/artifacts and can be aligned with player source facts. |
| Where do monitor/timeline/wave/filmstrip get facts? | Timeline uses `CarrierPositionChanged`, `ExecutionRangeChanged`, and boundary events. Monitor consumes already-confirmed frame headers/payload descriptors. Wave and filmstrip plan from saved `Snapshot` metadata/artifacts. |
| Where does transport resolver produce private bindings? | `qnc-transport-resolver` maps public `qnc://...` URIs to local/network endpoints. `qnc-player-launcher::SourceTransportBinding` turns that into a private `MediaBinding`. |
| Where does QNC cross from workflow into backend/player? | At `qnc-player-input`/`PreparedInput`, then `qnc-player-launcher`, then `qnc-player-client` session/command transport. |

Important existing QNC invariants to preserve:

- `PreparedInput::media()` selects picture representation.
- `PreparedInput::audio_media()` stays original.
- `CommandEnvelope` carries `contract_version`, `session_id`, `request_id`,
  `source_generation`, `sequence`, and command.
- stale source generation rejects before execution.
- `VideoFrameSubmitted` is not `FramePresented`.
- `FramePresented` requires actual presentation evidence.
- raw local paths are private bindings, not public UI/player state.

## Future QNC->QGS Bridge Location

Recommended QNC-side location:

```text
qnc/player/qgs_bridge/
  client.rs
  bridge_session.rs
  command_mapper.rs
  descriptor_mapper.rs
  event_mapper.rs
  snapshot_mapper.rs
  fault_mapper.rs
  generation.rs
  private_binding.rs
```

If QNC keeps the current crate-per-module style, the same boundary can become a
small crate such as:

```text
crates/qnc-qgs-bridge/
```

The bridge should depend on QNC application-side contracts and the future
shared `qnc-qgs-contract` crate when it exists. Until then, it should use local
adapter shapes that mirror Blocks P, Q, and R. QGS should not import QNC crates.

## Bridge Responsibilities

The bridge should:

- accept `qnc-player-input::PreparedInput` and QNC private transport bindings.
- build the QNC/QGS descriptor shape planned in Block P.
- map `PreparedInput` into QGS-compatible source mode and stream layout facts.
- preserve `ProxyPreview` and `OriginalMedia` source modes.
- enforce original MXF audio as authoritative in both modes.
- preserve discrete original MXF mono lanes.
- pass opaque private binding references to the backend side.
- send generation-checked command envelopes to QGS.
- track `session_id`, generation, active source, and last public snapshot.
- receive QGS replies, snapshots, events, faults, and recovery records.
- project QGS facts into QNC passive view inputs.
- keep raw private paths out of public UI-facing output.
- keep device/realtime/display/audio truth boundaries visible.

## What Bridge Must Not Own

The bridge must not:

- own QGS runtime internals.
- own QNC UI rendering, forms, DB, or workflow.
- decode media.
- perform device output.
- infer backend readiness from UI state.
- treat diagnostic presenter/audio evidence as production output.
- collapse original mono lanes into stereo runtime truth.
- turn proxy AAC into authoritative audio.
- duplicate the Broadcast Player runtime logic inside QNC.
- become IPC, process launcher, or UI integration by itself.

## Bridge API Shape

Planned QNC-side types:

| Type | Purpose |
| --- | --- |
| `QncQgsBridge` | Long-lived application bridge facade owned by QNC player/controller code. |
| `QncQgsBridgeSession` | One active QGS-backed Broadcast Player session. |
| `QncQgsBridgeConfig` | Contract version, backend mode, default source mode, and feature flags. |
| `QncQgsBridgeCommand` | QNC-side command enum before mapping to the shared command envelope. |
| `QncQgsBridgeReply` | Accepted/rejected result plus snapshot, events, fault/recovery facts. |
| `QncQgsBridgeSnapshot` | Public-safe current runtime snapshot for QNC views. |
| `QncQgsBridgeEvent` | Public-safe event projection for QNC modules. |
| `QncQgsBridgeFault` | Public-safe backend fault/recovery projection. |
| `QncQgsPrivateBindingRef` | Opaque resolver binding handle, never a public path. |

Planned methods:

| Method | Behavior |
| --- | --- |
| `open_session(prepared_input, private_bindings)` | Create a bridge session from QNC-prepared facts and opaque private resolver bindings. |
| `close_session()` | Close the active bridge session and invalidate outstanding generations. |
| `prepare()` | Ask QGS to validate/bind source facts and prepare backend readiness facts. |
| `cue(frame)` | Cue a frame without claiming presentation. |
| `preroll()` | Ask QGS to prepare the bounded window around the cue/anchor. |
| `play()` | Start only when QGS reports ready; no media discovery on play. |
| `play_for(frames)` | Operator/test bounded run helper only. |
| `pause()` | Pause transport state. |
| `seek(frame)` | Seek/cue target frame and require preroll before play if needed. |
| `stop()` | Stop transport while preserving source until unload/close. |
| `unload()` | Release active source/session facts. |
| `snapshot()` | Return the current public-safe QGS snapshot. |
| `drain_events()` | Return ordered public-safe events since last drain. |

Session state:

- `session_id`
- `generation`
- `current_snapshot`
- `active_source_id`
- `public_source_uri`
- `qgs_backend_status`
- `last_event_sequence`
- `private_path_exposed` safety flag

## Command Mapping Table

| QNC intent | QNC source | QGS command | Payload | Generation behavior | Notes |
| --- | --- | --- | --- | --- | --- |
| open/load selected clip | `PreparedInput` + private binding from launcher/resolver | `LoadPreparedInput` | prepared descriptor and opaque binding ref | accepted load increments generation | QNC owns DB/work settings; QGS validates descriptor/runtime facts. |
| preload source | `BroadcastPlayerProtocolCommand::PreloadSource` or preselection | `Prepare` or future `PreloadSource` | public source id plus descriptor facts | accepted mutation increments generation | Keep preloading distinct when QGS adds multi-source support. |
| set active source | `SetActiveSource { source_id }` | `LoadPreparedInput` for current single-source path, future `SetActiveSource` | source id | accepted mutation increments generation | Do not invent second input model; consume Block Q descriptor. |
| set active range | `SetPlaybackRequest` / execution range | current QGS active range in descriptor/runtime command | half-open frame range | accepted mutation increments generation | Must validate against source duration. |
| prepare | player prepare/readiness intent | `Prepare` | empty | accepted mutation increments generation | Does backend validation and readiness planning, not UI work. |
| cue | `CueFrame { frame, present_frame }` | `Cue` | frame | accepted mutation increments generation | `present_frame` cannot imply real presentation. |
| prepare anchor / preroll | readiness window intent | `Preroll` | optional target frame | accepted mutation increments generation | Ready only after QGS preroll/prepared-window facts say ready. |
| play | `Play` | `Play` | optional bounded test frame count | accepted mutation increments generation | Play must reject before ready and must not discover media. |
| pause | `Pause` | `Pause` | empty | accepted mutation increments generation | Transport state only. |
| seek | step/cue/operator seek | `Seek` then `Preroll` | target frame | each accepted mutation increments generation | Seek after completed requires documented reset/new session behavior. |
| stop | `Stop` | `Stop` | empty | accepted mutation increments generation | Stop does not claim device output. |
| unload/close source | `UnloadSource` or selection close | `Unload` / `close_session()` | empty | accepted mutation increments generation; close invalidates session | Public snapshot must hide private bindings. |
| snapshot/query | `SessionRequest::State` | `Snapshot` | empty | does not increment generation | Safe observer path; no mutation. |
| stale command | any command with old generation | reject before QGS mutation | original payload ignored | generation unchanged | Emit/publicly expose command rejection only. |

## Snapshot/Event Mapping Table

| QGS bridge fact | QNC consumer | Projection rule |
| --- | --- | --- |
| `session_id`, generation, sequence | QNC player client/controller | Preserve for stale command rejection and ordered events. |
| `status` | transport controls/status labels | Use to enable/disable operator actions; do not infer media facts beyond snapshot. |
| `source_loaded`, `source_id`, `public_source_uri` | project/player state | Public URI may be shown; private path must not appear. |
| `source_mode` | monitor/status labels | `ProxyPreview` or `OriginalMedia`; do not say proxy-only. |
| picture representation | monitor/status labels | Proxy MP4 for `ProxyPreview`, original MXF for `OriginalMedia`. |
| authoritative audio source | wave/audio status | Must remain original MXF. Proxy AAC is diagnostic/fallback only. |
| broadcast audio model | wave/audio lane views | Preserve discrete mono lanes A1-A4/source track identity. |
| active range | timeline | Half-open range projected to QNC timeline. |
| current frame | timeline/transport | Use carrier/position-style facts for playhead. |
| current audio sample range | wave/audio status | Sample-accurate original-audio cursor/range. |
| prepared window | transport/buffer indicator | Operator can see ready window; UI does not own readiness. |
| payload readiness | status/warnings | Distinguish runtime accounting, payload ready, device ready, real presentation. |
| device/backend status | status/warnings | Preserve non-claims for display, production audio, realtime, A/V sync. |
| faults/recovery | status/recovery UI | QNC presents; QGS provides backend fault/recovery facts. |
| `SourceLoaded`/`PreparedInputAccepted` | project/player state | Confirms backend accepted prepared source facts. |
| `Cued`/`Seeked`/`PrerollReady` | timeline/transport | Updates position/readiness without claiming presentation. |
| `Started`/`Paused`/`Stopped`/`Unloaded` | transport controls | Public transport state. |
| `Ticked`/position events | timeline | Playhead progression from runtime facts. |
| `CommandRejected` | operator feedback | Preserve reason; no mutation on stale generation. |
| `FramePresented` | monitor/status | Only pass through if QGS has real presentation evidence. Current bridge should normally show none. |

## Private Binding / Resolver Plan

QNC should keep resolver ownership.

Recommended path:

```text
QNC public media URI
  -> qnc-transport-resolver
  -> local/network private endpoint
  -> QNC bridge opaque binding ref
  -> QGS backend source binding
```

Rules:

- Public `qnc://...` URIs may cross into snapshots/events.
- Private local paths, tokens, and network credentials must not appear in public
  QNC-facing output.
- QGS may receive private binding material only through a backend-private
  binding object or opaque reference.
- QGS replies must include a `private_path_exposed=false` safety fact.
- Any public snapshot/event/fault that exposes a local path must be treated as
  a bridge bug and rejected before UI projection.

## Generation / Session Behavior

The QNC bridge should preserve the existing QNC generation model and the QGS
Block R generation behavior:

- Opening or replacing a source creates or advances the active source
  generation.
- Mutating accepted commands advance generation.
- Snapshot/query commands do not advance generation.
- Rejected commands do not mutate state and do not advance generation.
- Commands may carry `expected_generation`.
- If `expected_generation` mismatches the current generation, reject before
  runtime mutation.
- The bridge stores the latest accepted generation and includes it in following
  command envelopes.
- QNC UI/client code may observe readiness and events, but it must not own the
  media clock policy or backend readiness.

## In-Process-First Plan

The safest next work is in-process first, before IPC:

1. **Fixture compatibility:** build QNC `PreparedInput` fixture examples and
   verify that Block Q descriptor mapping accepts the same field semantics.
2. **QNC-side bridge skeleton:** create a QNC bridge module that calls an
   in-process QGS adapter in tests/development, without network/process IPC.
3. **Command script parity:** drive load, prepare, cue, preroll, play, pause,
   seek, stop, and unload through the bridge and compare public snapshots.
4. **Passive projection mocks:** feed bridge snapshots/events into QNC monitor,
   timeline, wave, and status projection code without moving those modules.
5. **Shared contract crate:** only after field names stabilize, create the
   lightweight `qnc-qgs-contract` crate planned in Block P.
6. **IPC/process boundary:** add process or socket transport only after the
   in-process bridge proves session/generation/event behavior.

This keeps the bridge modular. It avoids turning QGS into a QNC application or
turning QNC into a backend engine.

## Risks

| Risk | Mitigation |
| --- | --- |
| Bridge duplicates QGS runtime behavior | Keep bridge as mapper/session facade only; call QGS control surface for backend facts. |
| QNC UI infers readiness from stale state | Require generation-checked snapshots and reject stale commands before mutation. |
| Private local paths leak to UI | Keep opaque private binding refs and assert `private_path_exposed=false` in public output. |
| Proxy AAC becomes convenient runtime audio | Contract must require original MXF audio authority in both source modes. |
| Stereo/desktop helper semantics leak into runtime truth | Preserve discrete original mono lanes in descriptor, snapshots, and wave projections. |
| Diagnostic output is mistaken for production output | Keep evidence levels and status fields separate: test, payload, device, presentation, realtime. |
| IPC is added before field stability | Use in-process bridge acceptance first; defer transport. |
| Existing QNC backend implementation shapes QGS too tightly | Reuse QNC contract/behavior concepts, not legacy backend internals. |

## Recommended Next Block

Recommended next milestone:

```text
M2 Block T - QNC PreparedInput Fixture Compatibility
```

Goal:

- create representative QNC `PreparedInput` fixture records for `ProxyPreview`
  and `OriginalMedia`.
- map them through the existing QGS Block Q descriptor path.
- verify source identity, private binding opacity, original MXF audio authority,
  discrete mono lanes, project audio, active range, and generation-safe command
  setup.
- avoid IPC, UI integration, device output, realtime playback, and A/V sync.

This is the lowest-risk step before building an actual QNC-side
`qgs_bridge` module because it proves the bridge input contract against real QNC
field semantics.

## Verification Matrix Note

Block S does not add a new runtime evidence row. It is a bridge plan artifact,
not runtime implementation or device evidence. Existing `ContractPlanEvidence`,
`DescriptorMappingEvidence`, and `QncControlSurfaceShapeEvidence` remain the
current verification anchors until a QNC-side bridge prototype exists.

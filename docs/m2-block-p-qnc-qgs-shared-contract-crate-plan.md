# M2 Block P - QNC/QGS Shared Contract Crate Plan

Block P defines the field-level plan for a future shared contract between QNC
and QGS.

This is a contract planning block. It does not create the crate, implement IPC,
change QNC UI, change QGS runtime behavior, add device backends, certify
realtime playback, or create a monolith.

Update after M2 Block R: QGS now has an in-process
`QgsQncControlSurface` that implements the QGS-side shape of these command,
reply, snapshot, event, fault, recovery, generation, privacy, and device-status
fields. That implementation is intentionally not the shared
`qnc-qgs-contract` crate, and it does not add IPC or QNC UI integration.

Update after M2 Block S: the QNC-side bridge prototype plan consumes this
planned field surface from the QNC side. The bridge remains a QNC adapter that
maps QNC `PreparedInput`, private resolver bindings, commands, snapshots,
events, and faults to the QGS backend shape. It is still not the shared
`qnc-qgs-contract` crate and does not implement IPC, QNC UI integration,
realtime playback, A/V sync, or device output.

## Purpose

QNC is the application. It owns UI, forms, DB, workflow, public media identity,
operator intent, and passive views.

QGS is the backend engine. It owns Broadcast Player runtime behavior, media
validation, original/proxy mapping, prepared state, device boundary facts,
snapshots, events, faults, and verification truth.

The shared contract must be narrow:

```text
QNC -> command / descriptor / session intent -> QGS
QGS -> reply / snapshot / event / fault facts -> QNC
```

The shared contract must not expose QGS internals to QNC, and it must not copy
QNC UI/DB/workflow code into QGS.

## Scope

Block P plans:

- contract source mapping
- future crate scope
- proposed field names
- command/reply/snapshot/event/fault surfaces
- source identity and private binding rules
- ownership and versioning
- risks and next block

Block P does not implement:

- `qnc-qgs-contract`
- IPC
- QNC UI changes
- QGS runtime features
- Wayland/Vulkan, X11, DRM/KMS
- production PipeWire audio
- realtime playback
- A/V sync
- export/render

## Current Contract Sources Inspected

No local QNC checkout was present at `/home/miro/Development/qnc` or
`/home/miro/Development/QNC`. The existing audit checkout was used:

```text
/tmp/qnc-audit-1790707209
```

### Contract Source Summary

| Category | QNC source | Useful types / fields | Dependencies | Portable? | Use |
| --- | --- | --- | --- | --- | --- |
| Prepared input / media descriptor | `qnc-player-input` | `PreparedInput`, `Representation`, `StreamLayout`, `VideoInput`, `AudioChannel`, `ProjectAudio`, `PlaybackInput` | QNC work settings/media records/metadata | Partially | Adapt. Keep shape, not DB readers. |
| Source identity / resolver binding | `qnc-source-contract`, `qnc-transport-resolver` | `SourceReference`, `ResolvedResource`, QNC URI parsing, local/network endpoint split | QNC contracts, path/network resolver | Partially | Reference/adapt public URI and opaque binding concepts. Avoid raw paths in public output. |
| Frame range / timebase | `qnc-player-contract`, `qnc-frame-timebase` | `FrameRange`, `FrameNumber`, `Timebase` | Light | Yes | Reuse/adapt field semantics. |
| Stream layout / project audio | `qnc-player-input`, `qnc-audio-output` | `StreamLayout`, `AudioChannel`, `ProjectAudio`, `ChannelMap`, `Format` | Light to medium | Yes if separated | Adapt. Preserve mono lanes; channel map is routing only. |
| Command envelope | `qnc-player-contract::envelope`, `interface::protocol` | `CommandEnvelope`, `BroadcastPlayerProtocolCommand`, `request_id`, `session_id`, `source_generation`, `sequence` | Serde, player model | Yes | Reference/adapt; add QGS prepared-input/source-mode payloads. |
| Command reply | `qnc-player-contract::session` | `SessionReply = Result<EventEnvelope, String>` | Serde | Partially | Adapt into explicit reply with snapshot, mutation, fault, recovery. |
| Session generation | `qnc-player-contract::session`, `envelope` | `SessionQuery`, `source_generation`, sequence validation | Light | Yes | Reuse behavior. |
| Snapshot/passive view | QGS current runtime, QNC client/session query | QGS `QgsQncPassiveView`, `QgsBroadcastPlayerSnapshot`, QNC `SourceRuntime` | QGS/QNC local | Needs shared type | Adapt into explicit snapshot. |
| Event envelope | `qnc-player-contract::envelope`, `event`, `interface::protocol::event` | `EventEnvelope`, `BroadcastEvent`, `BroadcastPlayerProtocolEvent` | Serde, player model | Yes | Reference/adapt; keep submitted vs presented distinction. |
| Fault/error/recovery | QGS operational fault/recovery; QNC command rejection/error enums | `QgsOperationalFault`, recovery actions, `CommandRejected`, output `Code` enums | QGS local | Needs shared type | Define shared safe fault shape. |
| Device/output status | QGS Block H/I/K and device boundary types; QNC audio/video output config/telemetry | `QgsBroadcastPlayerDeviceStatus`, `OutputConfig`, audio `Telemetry`, video output config | Backend-adjacent | Contract only | Adapt status fields; avoid backend APIs. |
| Channel map / audio lane semantics | `qnc-audio-output::ChannelMap`, QGS original audio layout | `source_channels`, `output_channels`, QGS mono lane types | Light | Yes | Reuse semantics: no stereo collapse, routing is not mixdown. |

### Useful QNC Field Names

Useful names to keep compatible where possible:

- `contract_version`
- `session_id`
- `request_id`
- `source_generation`
- `sequence`
- `source_id`
- `source_runtime`
- `execution_range`
- `initial_frame`
- `start_frame`
- `end_frame`
- `timebase`
- `sample_rate_hz`
- `channel_count`
- `source_channels`
- `output_channels`

### QNC Sources To Avoid In Shared Contract

Avoid pulling these into a shared crate:

- DB readers / `SettingsReader`
- QNC form/layout/widget code
- concrete QNC backend runtime implementation
- PipeWire, Wayland, Vulkan, X11, DRM/KMS dependencies
- local `PathBuf` endpoints in public event/snapshot structs

## Future Shared Crate Purpose

Recommended future crate name:

```text
qnc-qgs-contract
```

Purpose:

- pure contract/model types only
- dependency-light
- serde-ready
- shared by QNC application bridge and QGS control surface when fields are
  stable enough

## What Belongs In The Crate

The future crate may contain:

- version constants
- source identity types
- private binding reference shape
- prepared input descriptor types
- source role/source mode enums
- stream/audio layout types
- frame/timebase/range types
- command request envelope
- command reply envelope
- runtime snapshot structs
- event envelope
- fault/recovery records
- device/evidence status enums

## What Must Not Belong

The future crate must not contain:

- actual local file paths in public structs
- QNC UI strings as primary state
- DB connection or DB read logic
- QNC workflow implementation
- QGS runtime state machine implementation
- media decode/extraction implementation
- PipeWire/Wayland/Vulkan/X11/DRM/KMS APIs
- qgs-test diagnostics
- presentation or audio output implementation

## Proposed Contract Version

Type:

```text
QncQgsContractVersion
```

Fields/constants:

| Field | Type | Proposed value | Notes |
| --- | --- | --- | --- |
| `contract_name` | string | `qnc-qgs-contract` | Stable crate/interface name. |
| `contract_version` | string | `m2-v1-draft` | Draft until QNC bridge acceptance exists. |
| `stability` | string/enum | `draft-internal` | Not a public/stable external API yet. |
| `minimum_qgs_m2_block` | string | `M2 Block P` | Field plan begins here. |
| `notes` | optional string | optional | Human migration note only. |

Rule:

- Version mismatch must reject command execution before mutation.

## Source Identity / Binding Plan

Types:

- `QncQgsSourceId`
- `QncQgsPublicSourceUri`
- `QncQgsPrivateBindingRef`
- `QncQgsSourceMode`
- `QncQgsSourceIdentity`

Proposed fields:

| Field | Type | Direction | Notes |
| --- | --- | --- | --- |
| `public_uri` | string | QNCToQGS / QGSToQNC | Public QNC URI. May cross boundary. |
| `source_id` | string | Bidirectional | Runtime-safe source id. Should not be a path. |
| `binding_ref` | optional opaque string | QNCToQGS | Private resolver binding handle. Not a path in public events. |
| `source_mode` | enum | QNCToQGS / QGSToQNC | `ProxyPreview` or `OriginalMedia`. |
| `original_media_id` | optional string | QNCToQGS | Public/safe original media identity. |
| `proxy_media_id` | optional string | QNCToQGS | Public/safe proxy identity. |
| `private_path_exposed` | bool | QGSToQNC | Must be `false` in public output. |

Rules:

- public URI may cross the boundary.
- private binding reference may cross only as opaque backend binding.
- raw private local paths must not appear in snapshots/events.
- `ProxyPreview` means proxy MP4 video plus original MXF audio.
- `OriginalMedia` means original MXF video plus original MXF audio.

## Prepared Input Descriptor Plan

Type:

```text
QncQgsPreparedInputDescriptor
```

Proposed fields:

| Field | Type | Notes |
| --- | --- | --- |
| `source_identity` | `QncQgsSourceIdentity` | Public and opaque private binding facts. |
| `selected_picture_representation` | enum | `Proxy` or `Original`. |
| `authoritative_audio_representation` | enum | Must be `Original`. |
| `source_mode` | enum | `ProxyPreview` or `OriginalMedia`. |
| `active_range` | optional `QncQgsFrameRange` | Half-open; may default to full source. |
| `original_video_role` | media role enum | Original video role. |
| `proxy_video_role` | media role enum | Proxy preview role if available. |
| `original_audio_role` | media role enum | Authoritative audio role. |
| `proxy_audio_role` | media role enum | Diagnostic/fallback only. |
| `stream_layout` | `QncQgsStreamLayout` | Video/audio inventory. |
| `project_audio` | `QncQgsProjectAudio` | Expected project output format. |
| `original_proxy_timing_compatible` | bool | Source association/timing check result. |
| `descriptor_revision` | u64 | Incremented by QNC descriptor builder. |

Rules:

- proxy AAC is diagnostic/fallback only.
- original audio is authoritative in every mode.
- original MXF mono lanes remain discrete.
- descriptor accepts public IDs and opaque bindings, not raw public paths.

## Timebase / Range Plan

Types:

- `QncQgsFrameRate`
- `QncQgsFrameRange`
- `QncQgsAudioSampleRange`
- `QncQgsMediaTime`

Proposed fields:

| Type | Fields | Rules |
| --- | --- | --- |
| `QncQgsFrameRate` | `numerator`, `denominator` | Non-zero denominator. Rational source timing. |
| `QncQgsFrameRange` | `start_frame`, `end_frame_exclusive` | Half-open. End must be greater than start. |
| `QncQgsAudioSampleRange` | `start_sample`, `end_sample_exclusive`, `sample_rate` | Half-open. Original audio sample domain. |
| `QncQgsMediaTime` | `media_time_nanos` or rational `numerator/denominator` | Prefer rational if frame/sample exactness is required. |

Rules:

- no UI/export FPS as runtime truth.
- QGS owns runtime timing facts.
- QNC may display timing but must not infer backend readiness from display time.

## Stream / Audio Layout Plan

Types:

- `QncQgsStreamLayout`
- `QncQgsAudioLane`
- `QncQgsProjectAudio`
- `QncQgsChannelKind`
- `QncQgsAudioLaneRole`

Proposed fields:

| Field | Type | Notes |
| --- | --- | --- |
| `audio_sample_rate` | u32 | 48000 for Sony FX6 sample path. |
| `audio_bit_depth` | u16 | 24 for current original MXF audio. |
| `audio_lane_count` | u16 | Count of authoritative original mono lanes. |
| `lanes` | list | One record per original mono lane. |
| `lane_index` | u16/u32 | Contract lane number. |
| `source_track_index` | u32 | Original stream/track identity. |
| `channel_kind` | enum | `Mono` for current broadcast audio truth. |
| `role` | enum | `OriginalMonoLane`, future roles if needed. |
| `authoritative` | bool | `true` for original MXF lanes. |

Rules:

- channel map is routing, not mixdown.
- no stereo collapse.
- desktop monitor helpers are not production routing.
- repeated or folded output must be explicit device-boundary policy, not
  contract audio truth.

## Command Request Plan

Type:

```text
QncQgsCommandRequestEnvelope
```

Proposed envelope fields:

| Field | Type | Notes |
| --- | --- | --- |
| `command_id` | string/u64 | Required. Stable per request. |
| `session_id` | optional string | Required once a session exists. |
| `expected_generation` | optional u64 | If present, mismatch rejects before mutation. |
| `command_kind` | enum | See below. |
| `payload` | tagged payload | Only fields for the selected command. |
| `requested_at` | optional timestamp | Observability only. |
| `client_tag` | optional string | Debug/operator correlation only. |

Command kinds:

- `LoadPreparedInput`
- `Prepare`
- `Cue`
- `Preroll`
- `Play`
- `Pause`
- `Seek`
- `Stop`
- `Unload`
- `Snapshot`

Payloads:

| Command | Payload |
| --- | --- |
| `LoadPreparedInput` | `descriptor_ref` or inline descriptor. Decision remains unstable. |
| `Prepare` | empty or target facts. |
| `Cue` | `frame`. |
| `Preroll` | optional `target_frame`. |
| `Play` | optional `frame_count`; no media discovery. |
| `Pause` | empty. |
| `Seek` | `frame`. |
| `Stop` | empty. |
| `Unload` | optional source id. |
| `Snapshot` | empty. |

Rules:

- `Play` must not open media, decode, fill queues, or perform first usable
  preroll.
- rejected stale-generation commands must not mutate state.

## Command Reply Plan

Type:

```text
QncQgsCommandReplyEnvelope
```

Proposed fields:

| Field | Type | Notes |
| --- | --- | --- |
| `command_id` | same as request | Required correlation. |
| `accepted` | bool | Whether command was accepted. |
| `generation_before` | u64 | Generation before handling. |
| `generation_after` | u64 | Generation after handling. |
| `status_before` | enum | Runtime status before command. |
| `status_after` | enum | Runtime status after command. |
| `state_mutated` | bool | Must be false for rejected commands. |
| `rejection_reason` | optional enum/string | QNC-safe. |
| `fault` | optional `QncQgsFault` | If command records a fault. |
| `recovery_action` | optional enum | If recovery is suggested. |
| `snapshot` | `QncQgsRuntimeSnapshot` | Always returned. |
| `events` | list `QncQgsEventEnvelope` or inline events | Events caused by command. |
| `private_path_exposed` | bool | Must be false. |

Rules:

- rejected commands return a snapshot.
- stale generation rejection is non-mutating.
- command acceptance is not presentation evidence.

## Runtime Snapshot Plan

Type:

```text
QncQgsRuntimeSnapshot
```

Proposed fields:

| Field | Type | Notes |
| --- | --- | --- |
| `generation` | u64 | Runtime/source generation. |
| `status` | enum | Empty, Loaded, Ready, Playing, Paused, Stopped, Failed, etc. |
| `source_loaded` | bool | Public fact. |
| `public_source_uri` | optional string | No local path. |
| `source_mode` | optional enum | `ProxyPreview` / `OriginalMedia`. |
| `picture_representation` | optional enum | Proxy/original. |
| `authoritative_audio_source` | enum/string | Original MXF. |
| `proxy_aac_authoritative` | bool | Must be false. |
| `broadcast_audio_model` | enum/string | Discrete original mono lanes. |
| `active_range` | optional frame range | Half-open. |
| `current_frame` | optional u64 | Logical frame. |
| `current_audio_sample_range` | optional sample range | Original audio range. |
| `media_time` | optional media time | Exact enough for UI. |
| `prepared_window` | optional range/window | Bounded readiness. |
| `video_payload_ready` | bool/status | Payload, not display. |
| `audio_payload_ready` | bool/status | Payload, not audible output. |
| `buffer_status` | enum/struct | Ready/underrun/etc. |
| `device_status` | `QncQgsDeviceStatus` | Backend output boundary status. |
| `active_faults` | list | QNC-safe faults. |
| `warnings` | list | QNC-safe warnings. |
| `private_path_exposed` | bool | Must be false. |

## Device Status Plan

Type:

```text
QncQgsDeviceStatus
```

Proposed fields:

| Field | Type | Notes |
| --- | --- | --- |
| `device_policy` | enum/string | Preview/diagnostic/headless/future production. |
| `selected_video_backend` | enum/string | e.g. file diagnostic, future Wayland/Vulkan. |
| `selected_audio_backend` | enum/string | e.g. PipeWire diagnostic/future production. |
| `qnc_os_display_target` | enum/string | Wayland + Vulkan. |
| `x11_target_status` | enum/string | Legacy/non-target. |
| `real_display_status` | enum | NotImplemented until real presenter. |
| `presenter_evidence_level` | evidence enum | Test/file/readback/visual verified. |
| `audio_device_status` | enum | Diagnostic/production status. |
| `visual_verified` | bool | False unless actual visual verification. |
| `realtime_verified` | bool | False unless timed proof. |
| `audio_device_verified` | bool | False unless verified narrowly and explicitly. |
| `av_sync_verified` | bool | False unless measured proof. |

Rules:

- QNC OS display target is Wayland + Vulkan.
- X11 is legacy/non-target.
- no real `FramePresented` without evidence.
- production audio remains false until verified.

## Event Envelope Plan

Type:

```text
QncQgsEventEnvelope
```

Proposed fields:

| Field | Type | Notes |
| --- | --- | --- |
| `sequence` | u64 | Ordered runtime event sequence. |
| `generation` | u64 | Runtime/source generation. |
| `event_kind` | enum | See below. |
| `command_id` | optional | If event was command-caused. |
| `status` | optional enum | Runtime status if changed/relevant. |
| `frame` | optional u64 | Frame fact if relevant. |
| `audio_sample_range` | optional sample range | Original audio range. |
| `prepared_window` | optional | If readiness changed. |
| `fault` | optional | Safe fault. |
| `recovery_action` | optional | Recovery suggestion. |
| `evidence_level` | optional | Evidence/non-claim level. |
| `private_path_exposed` | bool | Must be false. |

Event kinds:

- `SessionCreated`
- `SourceLoaded`
- `PreparedInputAccepted`
- `Cued`
- `PrerollReady`
- `Started`
- `Ticked`
- `Paused`
- `Seeked`
- `Stopped`
- `Unloaded`
- `CommandRejected`
- `FaultRecorded`
- `RecoverySuggested`
- `SnapshotReported`

Rules:

- `FramePresented` should only be added when the contract can carry actual
  presentation evidence.
- `VideoFrameSubmitted` and `FramePresented` remain distinct if both are
  later included.

## Fault / Recovery Plan

Type:

```text
QncQgsFault
QncQgsRecoveryAction
```

Proposed fields:

| Field | Type | Notes |
| --- | --- | --- |
| `fault_kind` | enum | Align with QGS fault/recovery model. |
| `severity` | enum | Info/warning/error/fatal style. |
| `scope` | enum | Source/session/device/runtime. |
| `recoverable` | bool | Whether an action exists. |
| `recovery_action` | enum | QNC-safe action. |
| `command_id` | optional | Command that triggered fault. |
| `state_mutated` | bool | Important for rejected commands. |
| `qnc_safe_code` | string | Stable UI/log code. |
| `qnc_safe_message` | optional string | No private paths. |
| `private_path_exposed` | bool | Must be false. |

Rules:

- QNC-safe messages may describe source IDs and public URIs, not local paths.
- recovery suggestions do not mutate state by themselves.

## Evidence / Non-Claim Plan

Type:

```text
QncQgsEvidenceLevel
```

Proposed values:

- `NotImplemented`
- `DiagnosticOnly`
- `TestBoundaryEvidence`
- `RuntimeBehaviorEvidence`
- `AssemblySurfaceEvidence`
- `ContractPlanEvidence`
- `ArchitectureAuditEvidence`
- `PayloadBound`
- `NativeBufferSubmissionVerified`
- `NativePostSubmitEvidence`
- `ManualSignalObservedContentUnverified`
- `ManualContentAudibilityPartiallyObserved`
- `VisualVerified`
- `AudioDeviceVerified`
- `RealtimeVerified`

Rules:

- diagnostic evidence must not imply production output.
- file/readback/test presenter evidence must not imply real display.
- PipeWire diagnostic evidence must not imply full audio playback.
- manual signal evidence must not imply channel certification.
- realtime verification requires measured timed proof.

## Mapping Tables

### Table 1 - Contract Type Mapping

| Contract type | QNC source concept | QGS source concept | Direction | Notes |
| --- | --- | --- | --- | --- |
| Contract version | `qnc-player-contract::VERSION`, `CommandEnvelope.contract_version` | Existing QGS contract strings | Bidirectional | Needs new `m2-v1-draft` naming. |
| Source identity | `SourceRuntime.source_id`, `SourceReference`, resolver URI | `QgsPreparedSourceIdentity` | Bidirectional | Public URI plus runtime source id. |
| Private binding | `ResolvedResource`, local binding maps | `QgsPreparedMediaBinding` | QNCToQGS | Opaque handle only in public contract. |
| Source mode | `Representation`, `PlaybackInput` | `QgsInputPlanSourceMode` | QNCToQGS | Needs explicit `ProxyPreview` / `OriginalMedia`. |
| Prepared input descriptor | `PreparedInput` | `QgsPreparedInputDescriptor` | QNCToQGS | Needs adapter from QNC DB/work settings. |
| Timebase/range | `Timebase`, `FrameRange` | `RationalRate`, frame clock/range types | Bidirectional | Half-open ranges. |
| Audio layout | `StreamLayout`, `AudioChannel`, `ChannelMap` | `QgsPreparedAudioLayout`, original audio tracks | QNCToQGS | Preserve mono lanes. |
| Command request | `CommandEnvelope`, `BroadcastPlayerProtocolCommand` | `QgsQncCommandEnvelope`, operational commands | QNCToQGS | Needs prepared input/source mode payloads. |
| Command reply | `SessionReply`, `EventEnvelope` | `QgsSessionCommandResult`, snapshots/events | QGSToQNC | Make explicit accepted/mutated/fault fields. |
| Runtime snapshot | QNC client passive state concepts | `QgsQncPassiveView`, `QgsBroadcastPlayerSnapshot` | QGSToQNC | Needs stable shared fields. |
| Event envelope | `EventEnvelope`, `BroadcastPlayerProtocolEvent` | `QgsQncEventEnvelope`, operational events | QGSToQNC | Keep evidence/non-claims. |
| Fault/recovery | command rejection, output error codes | `QgsOperationalFault` | QGSToQNC | Needs QNC-safe codes. |
| Device/evidence status | output config/telemetry, protocol events | QGS verification/device status | QGSToQNC | Do not overclaim. |

### Table 2 - Field Mapping

| Contract field | Existing QNC field/source | Existing QGS field/source | Status |
| --- | --- | --- | --- |
| `contract_name` | none | doc/runtime strings | NeedsQGSField |
| `contract_version` | `contract_version`, `VERSION` | descriptor contract strings | NeedsAdapter |
| `session_id` | `SessionQuery.session_id`, `CommandEnvelope.session_id` | session/control runtime ids | Ready |
| `command_id` | `request_id` | `QgsSessionCommandId`, command envelopes | NeedsAdapter |
| `expected_generation` | `source_generation` | `QgsRuntimeEventGeneration`, session generation | Ready |
| `public_uri` | `SourceReference`, QNC URI | `QgsPreparedSourceIdentity.public_uri` | Ready |
| `binding_ref` | resolver local binding | `QgsPreparedMediaBinding` | NeedsAdapter |
| `source_mode` | representation/playback input | `QgsInputPlanSourceMode` | Ready |
| `selected_picture_representation` | `PreparedInput.representation` | `QgsPlaybackRepresentation` | Ready |
| `authoritative_audio_representation` | `PreparedInput.audio_media()` implicit original | `QgsAudioRepresentation::Original` | Ready |
| `active_range` | `BroadcastPlaybackRequest.execution_range` | active range/cue types | Ready |
| `audio_lane_count` | `StreamLayout.audio_channels` | `QgsPreparedAudioLayout` | Ready |
| `proxy_aac_authoritative` | none explicit | QGS non-claim fields | NeedsQGSField |
| `device_status` | output configs/telemetry | `QgsBroadcastPlayerDeviceStatus` | NeedsAdapter |
| `private_path_exposed` | tests reject path fields | QGS passive view checks | Ready |
| `visual_verified` | none | verification matrix | Ready |
| `audio_device_verified` | none | verification matrix/device evidence | Ready |
| `av_sync_verified` | AV sync warnings only | verification matrix | NeedsAdapter |
| raw local path | resolver endpoint | private bindings only | DoNotExpose |

### Table 3 - Command Mapping

| Command kind | QNC equivalent | QGS current support | Gap |
| --- | --- | --- | --- |
| `LoadPreparedInput` | `LoadSource` plus `PreparedInput`/launcher | Phase 22/Block M descriptor loading | Needs final descriptor payload/ref decision. |
| `Prepare` | readiness/prebuffer behavior | Operational runtime prepare | Needs shared reply fields. |
| `Cue` | `CueFrame` | Supported | Rename/map payload. |
| `Preroll` | readiness/tick/prebuffer | Prepared window/tick prepare | Needs contract name and target semantics. |
| `Play` | `Play` | Supported; no work on Play | Ready. |
| `Pause` | `Pause` | Supported | Ready. |
| `Seek` | `CueFrame` or request initial frame | Supported as seek/cue behavior | Needs QNC naming decision. |
| `Stop` | `Stop` | Supported | Ready. |
| `Unload` | `UnloadSource` | Supported | Ready. |
| `Snapshot` | `SessionRequest::State` | Passive snapshots | Needs shared snapshot struct. |

### Table 4 - Snapshot Mapping

| Snapshot field | QNC consumer | QGS producer | Gap |
| --- | --- | --- | --- |
| `generation` | QNC bridge/client | QGS session/runtime generation | Ready. |
| `status` | transport buttons, timeline | operational snapshot | Needs stable enum names. |
| `source_loaded` | UI source state | operational snapshot | Ready. |
| `public_source_uri` | project/monitor/timeline labels | prepared identity | Ready. |
| `source_mode` | player controller, monitor labels | input plan | Ready. |
| `current_frame` | timeline/monitor | operational position | Ready. |
| `current_audio_sample_range` | wave/timing views | frame/sample mapping | Ready. |
| `prepared_window` | transport readiness UI | prepared window | Ready. |
| `device_status` | warnings/status UI | device selector/status | NeedsAdapter. |
| `active_faults` | recovery UI | fault snapshot | Needs stable code list. |
| `private_path_exposed` | contract safety check | passive view | Ready. |

### Table 5 - Event Mapping

| Event kind | QNC consumer | QGS producer | Gap |
| --- | --- | --- | --- |
| `SessionCreated` | player controller | session/control surface | Needs shared session id policy. |
| `SourceLoaded` | UI/player state | transport/operational runtime | Ready. |
| `PreparedInputAccepted` | player controller | input/assembly path | Needs explicit event. |
| `Cued` | monitor/timeline | cue/transport | Ready. |
| `PrerollReady` | play button/readiness UI | prepared window | Ready. |
| `Started` | transport UI | play command | Ready. |
| `Ticked` | timeline/monitor | operational tick | Ready. |
| `Paused` | transport UI | pause command | Ready. |
| `Seeked` | timeline/monitor | seek command | Ready. |
| `Stopped` | transport UI | stop command | Ready. |
| `Unloaded` | source UI | unload command | Ready. |
| `CommandRejected` | command UI/log | command executor | Ready. |
| `FaultRecorded` | recovery UI/log | fault/recovery module | Ready with stable codes pending. |
| `RecoverySuggested` | recovery UI | fault/recovery module | Ready with stable actions pending. |
| `SnapshotReported` | passive refresh | snapshot projection | Needs final shape. |

## Ownership / Versioning Recommendation

### Where The Future Crate Should Live

Preferred:

```text
qnc-qgs-contract
```

as a small shared crate in a workspace or shared dependency location accessible
to both QNC and QGS.

During M2, do not create it yet. First stabilize a QGS-side
`qnc_control_surface` shape using this plan and acceptance docs.

### Version Ownership

Recommended ownership:

- QGS owns backend field truth and evidence semantics.
- QNC owns application/workflow needs.
- Version bumps require both sides to update mapping tests.

Compatibility rules:

- `draft-internal` versions may break between M2 blocks.
- once stable, additive fields require default/optional compatibility.
- removing/renaming fields requires major contract bump.
- unknown fields should be rejected in strict internal tests.
- stale session/generation commands reject without mutation.

### Same Crate vs Generated Schema

Recommendation:

- start with Rust types and serde-ready design.
- later generate JSON schema for QNC UI/client validation if needed.
- avoid generated-only schema before Rust field semantics settle.

### Serde Policy

Recommendation:

- plan for serde from the start.
- keep serde dependency optional only if it causes packaging friction.
- use `deny_unknown_fields` once the contract is implemented.

### Privacy / Internals Rules

The shared crate should enforce or make easy to test:

- raw paths do not appear in public output.
- QGS runtime internals do not appear in public structs.
- private binding refs are opaque.
- UI state is not backend truth.
- diagnostic evidence is not production verification.

## Risks

- Freezing the contract too early could preserve wrong field names.
- Duplicating `qnc-player-contract` without explicit mapping could create two
  player protocols.
- QGS could leak private paths through snapshots/events.
- QNC could treat diagnostic evidence as production output.
- QNC UI could infer readiness instead of reading QGS readiness.
- QGS could import QNC UI/DB dependencies.
- The shared crate could accidentally pull in PipeWire/Wayland/Vulkan/backend
  dependencies.
- Generation mismatch handling could mutate state if not tested.
- Version drift between QNC and QGS could be hard to diagnose without strict
  version fields and mapping tests.

## Recommended Next Block

Recommended:

```text
M2 Block Q - QNC PreparedInput to QGS Descriptor Mapping
```

Reason:

- `PreparedInput` is the first real application/backend bridge.
- It proves QNC-owned DB/work-settings/media identity can become QGS-owned
  backend runtime input without importing QNC UI or DB into QGS.
- It can validate source identity, source mode, original/proxy timing,
  original audio lane inventory, and private binding rules before IPC exists.

Follow-up after Block Q:

```text
M2 Block R - QGS QNC Control Surface Shape Implementation
```

That block can implement the command/reply/snapshot/event shapes once the input
descriptor mapping is proven.

## Block Q Follow-Up

M2 Block Q implements the first QNC-to-QGS descriptor bridge using QGS-side
QNC-like adapter types. It maps QNC `PreparedInput`-shaped source identity,
selected representation, stream layout, project audio, original/proxy timing,
and private binding facts into `QgsPreparedInputDescriptor`, `QgsInputPlan`, and
`QgsBroadcastPlayerAssembly`.

The final shared crate still does not exist. Block Q proves the descriptor
mapping shape that the future `qnc-qgs-contract` crate should preserve.

## Non-Claims

Block P does not claim:

- a shared crate exists
- IPC exists
- QNC UI integration exists
- QGS runtime behavior changed
- real display output exists
- production audio output exists
- realtime playback is verified
- A/V sync is verified
- export/render exists

It only records the shared contract field plan for future QNC/QGS integration.

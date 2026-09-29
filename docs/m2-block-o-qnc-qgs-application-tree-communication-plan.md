# M2 Block O - QNC/QGS Application Tree and Communication Plan

Block O designs how the QNC application should be organized when QGS becomes
the backend Broadcast Player engine.

This is an architecture and planning block. It does not implement IPC, QNC UI,
runtime behavior, Wayland/Vulkan, production PipeWire audio, export/render, or a
new monolithic player.

Update after M2 Block S: the QNC-side bridge shape is now planned in
`docs/m2-block-s-qnc-bridge-prototype-plan.md`. Block S keeps the bridge in QNC,
not QGS, and defines how QNC should map `PreparedInput`, private resolver
bindings, generation-checked commands, snapshots, events, and faults into the
QGS Broadcast Player backend surface.

## Purpose

The target product shape is:

```text
QNC application
  owns UI, forms, DB, workflow, public media identity, operator intent

QGS backend engine
  owns Broadcast Player runtime, media validation, source modes, prepared
  runtime state, device boundary policy, snapshots, events, and backend facts
```

QNC and QGS should meet through a narrow command/event/snapshot contract. QNC
must not reach into QGS runtime internals, and QGS must not import QNC UI,
project DB, workflow, or form code.

## Source Inspected

No local QNC checkout was present at `/home/miro/Development/qnc` or
`/home/miro/Development/QNC`. The public repository was inspected from:

```text
https://github.com/mironik/QNC
temporary audit checkout: /tmp/qnc-audit-1790707209
```

Current QGS references inspected:

- `docs/m2-block-n-qnc-broadcast-engine-reuse-parity-audit.md`
- `docs/m2-block-m-broadcast-player-modular-runtime-assembly.md`
- `docs/m2-integration-block-k-broadcast-player-running-runtime-demo.md`
- `docs/m2-runtime-lego-module-architecture.md`
- `crates/qgs-media-runtime/src/lib.rs`
- `tools/qgs-test/src/main.rs`

QNC references inspected:

- root `Cargo.toml`
- `apps/qnc-app`
- `apps/qnc-ingest`
- `apps/qnc-project`
- `contracts/applications`
- `contracts/databases`
- `contracts/modules`
- `contracts/ui`
- player/input/client/launcher/contract crates
- broadcast-player and broadcast-engine crates
- audio/video output crates
- monitor/timeline/wave/filmstrip crates
- DB/media/work settings/source crates
- ingest/project/application crates

## Current QNC Tree Summary

The current QNC repository already has useful separation, but it is organized as
many crates rather than as a single visible application tree. The categories
below describe the existing responsibilities.

| Category | Current QNC locations | Responsibility |
| --- | --- | --- |
| App shell | `apps/qnc-app`, `contracts/applications/shell.application.json`, `contracts/ui/shell.layout.json`, `qnc-shell-desktop-api`, `qnc-application-catalog` | Shell and application catalog entry points. |
| Forms/UI | `apps/qnc-ingest`, `apps/qnc-project`, `qnc-ingest-desktop`, `qnc-ingest-desktop-adapter`, `qnc-project-desktop`, `qnc-project-desktop-adapter`, `qnc-ui-kit`, `contracts/ui/*.layout.json` | Desktop forms, widgets, themes, layout contracts, form-specific adapters. |
| DB/media records/work settings | `qnc-db-contract`, `qnc-media-records`, `qnc-media-record-db`, `qnc-work-settings`, `qnc-project-store`, `qnc-ingest-store`, `contracts/databases/*.json` | Persistent project/media/work settings data. |
| Workflow/project/story/rundown | `qnc-ingest-application`, `qnc-ingest-work-plan`, `qnc-ingest-catalog`, `qnc-ingest-select`, `qnc-project-store`, `qnc-project-close`, `qnc-source-groups`, project/story application contracts | Application workflow, project context, ingest planning, selection, close behavior. |
| Player control/client | `qnc-player-contract`, `qnc-player-client`, `qnc-player-launcher`, `qnc-player-input`, `qnc-json-transport`, `qnc-player-frame-transport` | Prepared input, public protocol, launcher/client connection, command/session/event transport. |
| Monitor/timeline/wave/filmstrip | `qnc-monitor`, `qnc-player-timeline`, `qnc-timeline`, `qnc-timeline-assets`, `qnc-wave`, `qnc-wave-view`, `qnc-wave-worker`, `qnc-filmstrip`, `qnc-filmstrip-worker` | Passive UI/projection modules fed by media/player facts. |
| Transport resolver | `qnc-transport-resolver`, `qnc-source-contract`, `qnc-source-reader`, `qnc-source-index-contract`, `qnc-source-index-db` | Public source identity to private/local transport resolution. |
| Existing broadcast player/backend crates | `qnc-broadcast-player`, `qnc-broadcast-engine`, `qnc-audio-output`, `qnc-video-output`, `qnc-media-decode`, `qnc-ffmpeg-decode`, `qnc-gpu-raster`, `qnc-pixel-convert` | Current backend/player/media/output implementation areas that QGS is replacing or adapting behind contracts. |
| Shared contract/model crates | `qnc-contracts`, `qnc-player-contract`, `qnc-frame-timebase`, `qnc-media-stream`, `qnc-media-metadata`, `qnc-source-contract`, `qnc-db-contract`, module/application/database JSON contracts | Stable data contracts and model surfaces. |

## Proposed QNC Application Tree

The future QNC tree should read like an application, not as scattered backend
implementation pieces. This is a proposed organization, not a file move for
this block.

```text
qnc/
  apps/
    qnc-shell/
      main.rs
      app_registry/
      window_host/
    qnc-ingest/
      main.rs
    qnc-project/
      main.rs

  application/
    shell/
      application_catalog/
      desktop_api/
      shortcuts/
    project/
      project_store/
      project_close/
      work_settings/
    ingest/
      ingest_work_plan/
      ingest_catalog/
      ingest_select/
      ingest_store/
    story_rundown/
      story_model/
      rundown_model/

  forms/
    project/
      form/
      layout_contract/
      widgets/
      adapter/
    ingest/
      form/
      layout_contract/
      widgets/
      adapter/
    monitor/
      monitor_view/
    timeline/
      timeline_view/
    wave/
      wave_view/
    filmstrip/
      filmstrip_view/

  data/
    db_contracts/
    media_records/
    media_record_db/
    source_index/
    source_groups/
    source_reader/
    media_metadata/
    media_thumbnails/

  player/
    controller/
      operator_intent.rs
      active_session.rs
      command_router.rs
    qgs_bridge/
      client.rs
      command_mapper.rs
      event_mapper.rs
      snapshot_mapper.rs
      private_binding.rs
    prepared_input/
      qnc_prepared_input.rs
      descriptor_builder.rs
    passive_views/
      monitor_projection/
      timeline_projection/
      wave_projection/
      filmstrip_projection/

  contracts/
    application/
    database/
    ui/
    player_contract/
    qgs_contract/

  transport/
    resolver/
    json_transport/
    frame_transport/
```

The main change is conceptual clarity:

- forms live under `forms/<form-name>`
- workflow and DB live under `application/` and `data/`
- QNC player control lives under `player/controller`
- the QGS bridge is a small adapter, not the player itself
- passive UI projections remain in QNC
- QGS backend implementation does not live inside QNC forms or workflow

## Proposed QGS Backend Tree

QGS should remain a backend engine tree with small LEGO modules. This is the
shape QGS should move toward as APIs settle:

```text
qgs/
  crates/
    qgs-media-runtime/
      broadcast_player/
        assembly/
        control_core/
        operational_runtime/
        control_surface/
      input_descriptor/
      input_plan/
      transport/
      timing/
      preroll/
      prepared_window/
      video_payload/
      audio_payload/
      device_selection/
      presenter_boundary/
      audio_device_boundary/
      fault_recovery/
      event_surface/
      snapshot_projection/
      qnc_control_surface/

    qgs-mxf/
      metadata/
      pcm_extract/
      video_access_units/

    qgs-mp4/
      proxy_inspection/
      video_samples/

    qgs-codec-h264/
    qgs-software-video/
    qgs-vaapi/
    qgs-vulkan/
    qgs-audio-pipewire/

  tools/
    qgs-test/
      acceptance/
      operator_run/
      diagnostics/

  docs/
```

The QGS tree should make these boundaries visible:

- `broadcast_player/` is the product backend facade and assembly area
- `input_plan/` consumes QNC/QGS descriptors but does not read QNC DB
- `transport/` owns load/preload/set-active/range/cue/play legality
- `timing/` owns rational frame/sample facts
- `prepared_window/` owns bounded readiness
- `video_payload/` and `audio_payload/` bind backend payloads
- `device_selection/`, `presenter_boundary/`, and `audio_device_boundary/` own
  output boundary facts
- `qnc_control_surface/` owns the QNC-facing command/event/snapshot shape

## Communication Boundary

The QNC/QGS boundary should be shaped as command requests plus events and
snapshots. It should not expose raw local paths or module internals.

```text
QNC form/operator intent
  -> QNC player controller
  -> QNC QGS bridge
  -> QGS control surface
  -> QGS Broadcast Player assembly
  -> QGS runtime modules
  -> QGS snapshots/events/faults
  -> QNC QGS bridge
  -> QNC monitor/timeline/wave/filmstrip/forms
```

ASCII view:

```text
+-------------------+       +----------------------+       +----------------------+
| QNC form/module   | ----> | QNC player           | ----> | QNC QGS bridge       |
| Project/Ingest/UI |       | controller           |       | contract adapter     |
+-------------------+       +----------------------+       +----------+-----------+
                                                                     |
                                                                     v
+-------------------+       +----------------------+       +----------+-----------+
| QNC passive views | <---- | QNC event/snapshot   | <---- | QGS control surface  |
| monitor/timeline  |       | projection           |       | commands/snapshots   |
+-------------------+       +----------------------+       +----------+-----------+
                                                                     |
                                                                     v
                                                          +----------+-----------+
                                                          | QGS Broadcast Player |
                                                          | assembly/runtime     |
                                                          +----------------------+
```

### QNC Sends

QNC sends:

- prepared input / source descriptor
- public source identity
- private transport bindings through resolver output
- desired source mode: `ProxyPreview` or `OriginalMedia`
- command IDs
- optional expected session/runtime generation
- active range
- cue/seek frame requests
- load/preload/set-active/play/pause/stop/unload commands
- operator intent and selected profile

QNC must not send:

- UI state as backend truth
- raw private paths in public events
- proxy AAC as authoritative audio
- stereo folddown as runtime audio truth

### QGS Returns

QGS returns:

- command replies
- accepted/rejected status
- current generation
- passive snapshots
- event lists
- faults and recovery suggestions
- runtime status
- readiness and no-play-before-ready facts
- prepared window ranges
- media position
- frame/audio sample mapping
- source mode and media source roles
- device backend status
- presenter/audio boundary status
- evidence and non-claims

QGS must not return:

- raw local paths in public output
- `FramePresented` without presentation evidence
- `AudioDeviceVerified` from diagnostic-only audio evidence
- realtime/A-V sync claims without measured proof

## Ownership Table

| Area | QNC owns | QGS owns |
| --- | --- | --- |
| UI/forms | Forms, widgets, layouts, operator controls. | None. QGS may expose passive facts only. |
| DB/workflow | Project DB, media records, work settings, story/rundown/ingest workflow. | None. QGS consumes descriptors/bindings. |
| Public media identity | QNC URI, project/media IDs, resolver policy. | Public-safe source handles and private binding validation. |
| Prepared input | Building from QNC DB/work settings. | Validating and converting into QGS input plan. |
| Runtime mapping | Operator source-mode intent. | Original/proxy runtime mapping and media source roles. |
| Audio truth | Project audio expectations. | Original MXF audio, discrete mono lanes, payload/timing facts. |
| Video truth | User-selected representation intent. | ProxyPreview/original-media video payload paths and readiness. |
| Transport | UI command intent. | Load/preload/set-active/range/cue/play/pause/stop legality. |
| Timing | Displaying position and accepting operator commands. | Frame clock facts, frame-to-sample mapping, prepared windows. |
| Events/snapshots | Observation and UI projection. | Producing backend events, faults, snapshots, evidence levels. |
| Monitor/timeline/wave/filmstrip | Passive views and presentation of facts. | Source facts those views consume. |
| Device policy | User settings and device preference intent. | Backend selection, capability, readiness, boundary evidence. |
| Realtime/output claims | UI labels only from backend evidence. | Truthful backend verification state. |

## Shared Contract Layer Options

Only contract/model types should cross the boundary. Backend implementation and
UI code should not cross.

### Option A: Reuse / Adapt `qnc-player-contract`

Pros:

- already has QNC frame-based command, event, source, transport, and session
  vocabulary
- keeps QNC UI/client surfaces stable
- reduces duplicate public protocol concepts

Cons:

- it may carry QNC-era assumptions from the existing backend
- QGS cannot import QNC crates directly if the dependency direction becomes
  ambiguous
- QGS-specific evidence levels and source modes may need additions

Best use:

- use as reference contract vocabulary now
- migrate shared neutral types out if needed

### Option B: New `qnc-qgs-contract` Crate

Pros:

- explicit neutral boundary owned by the integration seam
- can contain only commands, replies, snapshots, events, descriptors, and
  evidence enums
- avoids importing either side's implementation details

Cons:

- adds another crate and migration surface
- duplicates parts of `qnc-player-contract` unless carefully derived
- requires QNC and QGS to agree on versioning

Best use:

- strong candidate once command/snapshot/event fields stabilize

### Option C: QGS-Side `qgs-qnc-control-surface` Crate

Pros:

- QGS can evolve the backend surface quickly
- avoids coupling QGS to QNC internals
- good place for acceptance tests against QNC-shaped commands

Cons:

- may look QGS-owned rather than shared
- QNC may still need a mirror adapter crate
- risk of drifting from QNC public protocol names

Best use:

- useful first implementation step if a shared crate is premature

### Recommendation

Use a two-step approach:

1. In the next block, define a stable QNC-compatible command/snapshot/event
   model in QGS documentation and tests, directly mapped to existing QNC
   `qnc-player-contract` concepts.
2. After fields settle, extract or introduce a small shared contract crate.

Recommended name for the shared crate when ready:

```text
qnc-qgs-contract
```

It should contain only:

- prepared input descriptor shape
- source identifiers and source modes
- command envelopes and replies
- runtime snapshots
- event envelopes
- fault/recovery records
- evidence/non-claim statuses

It should not contain:

- QNC UI code
- QNC DB readers
- QGS runtime implementation
- PipeWire/Wayland/Vulkan APIs
- qgs-test diagnostics

## Migration Plan

### Phase A - Keep QNC UI / Forms / DB As Is

Do not move forms or refactor workflow yet. QNC remains the application and
continues to own project DB, media records, work settings, and operator UI.

### Phase B - Add QNC-Side QGS Client / Bridge

Create a small QNC bridge module that knows how to:

- build QGS command requests
- hold session/generation state
- map QGS snapshots/events to QNC passive views
- hide private transport bindings from public UI output

### Phase C - Map QNC `PreparedInput` To QGS Descriptor

Use QNC media records and work settings to produce the QGS prepared descriptor:

- public source URI
- selected picture representation
- original/proxy association
- original audio lane inventory
- source mode
- private binding references

### Phase D - Route QNC Player Commands To QGS Control Surface

Map QNC operator commands to QGS:

- load/preload/set-active
- set active range
- cue/seek
- prepare anchor / tick prepare
- play/pause/stop
- close/unload

Generation checks should reject stale commands without mutating QGS runtime
state.

### Phase E - Feed QGS Snapshots / Events Back To QNC Passive Views

Use QGS outputs to drive:

- monitor status
- timeline playhead
- wave lane status
- filmstrip source selection
- transport button state
- warnings/fault recovery prompts

QNC passive views should observe facts, not infer backend readiness.

### Phase F - Replace Old QNC Backend Behind Same UI

Once the QGS bridge is stable, route QNC's player controller to QGS instead of
the old backend implementation. Keep the UI and workflow intact.

### Phase G - Add Real QGS Device Backends

Only after the control surface is stable:

- production PipeWire audio backend
- Wayland + Vulkan presenter
- optional DRM/KMS + Vulkan appliance presenter
- A/V sync and device-clock policy
- realtime verification

Do not target X11 for QNC OS.

## Risks

- Creating a new shared contract too early may fossilize unstable fields.
- Reusing QNC backend implementation directly may recreate the monolith.
- Letting QNC UI own backend readiness or timing would invert ownership.
- Letting QGS read QNC DB directly would blur application/backend boundaries.
- Passing raw paths through public events would break privacy and identity
  rules.
- Treating diagnostic PipeWire/readback/file-presenter evidence as production
  output would weaken verification discipline.
- Collapsing original MXF mono lanes into stereo would break broadcast audio
  truth.

## Recommended Next Block

Recommended:

```text
M2 Block P - QNC/QGS Shared Contract Crate Plan
```

Scope:

- list exact command envelope fields
- list exact reply/snapshot/event/fault fields
- decide versioning and ownership for a future `qnc-qgs-contract` crate
- map each field to current QNC `qnc-player-contract` and current QGS runtime
  fields
- keep this as contract planning, not IPC or runtime implementation

Alternative if implementation should begin sooner:

```text
M2 Block P - QNC PreparedInput to QGS Descriptor Mapping
```

The shared contract plan is slightly safer first because it prevents another
round of ad-hoc bridge fields before QNC and QGS start talking.

## Block P Follow-Up

M2 Block P defines the field-level shared contract plan for this communication
boundary. It names the future `qnc-qgs-contract` scope, proposed version fields,
source identity and private binding records, prepared input descriptor fields,
command/reply/snapshot/event/fault surfaces, evidence levels, ownership rules,
and mapping tables against current QNC and QGS concepts.

Block O remains the application-tree and ownership plan. Block P is the
contract-field plan that should guide the next bridge implementation work.

## Non-Claims

Block O does not implement:

- IPC
- QNC UI changes
- QNC form reorganization
- QGS runtime behavior
- Wayland/Vulkan
- X11
- DRM/KMS
- production PipeWire audio
- realtime playback
- A/V sync
- export/render

It only records the application tree, backend tree, ownership split, and
communication plan for QNC using QGS as its backend Broadcast Player engine.

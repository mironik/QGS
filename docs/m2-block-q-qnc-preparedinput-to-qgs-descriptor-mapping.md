# M2 Block Q - QNC PreparedInput To QGS Descriptor Mapping

Block Q implements the first QNC-to-QGS bridge shape:

```text
QNC PreparedInput-shaped data
  -> QGS-side QNC-like adapter
  -> QgsPreparedInputDescriptor
  -> QgsInputPlan
  -> QgsBroadcastPlayerAssembly
```

This is not IPC, QNC UI integration, a shared crate implementation, realtime
playback, A/V sync, Wayland/Vulkan, production PipeWire audio, export/render,
or a new monolithic player.

## Why This Exists

Block P defined the future shared contract plan. Block Q proves the first
practical mapping: QNC-owned prepared input facts can be consumed by QGS without
QGS importing QNC UI, DB, workflow, or application crates.

QNC remains owner of:

- UI/forms/workflow
- media records and work settings
- public media identity
- private transport resolution policy
- operator intent

QGS owns:

- descriptor validation
- source mode validation
- original/proxy runtime mapping
- original-audio authority
- prepared input plan generation
- backend runtime behavior after mapping

## QNC Concepts Inspected

The mapping is based on these QNC concepts:

- `qnc-player-input::PreparedInput`
- `Representation`
- `PlaybackInput`
- `StreamLayout`
- `VideoInput`
- `AudioChannel`
- `ProjectAudio`
- `qnc-source-contract::SourceReference`
- `qnc-transport-resolver` public URI/private endpoint separation
- `qnc-player-contract` frame range/timebase/session concepts
- `qnc-audio-output::ChannelMap`

QGS does not import those QNC crates. Block Q mirrors the portable field
semantics in QGS-side adapter types.

## QGS-Side Adapter Types

Added backend-neutral QGS-side compatibility shapes:

- `QgsQncPreparedInputLike`
- `QgsQncSourceIdentityLike`
- `QgsQncPrivateBindingLike`
- `QgsQncStreamLayoutLike`
- `QgsQncAudioChannelLike`
- `QgsQncProjectAudioLike`
- `QgsQncPlaybackInputLike`
- `QgsQncRepresentationLike`
- `QgsQncAudioRepresentationLike`
- `QgsQncFrameRangeLike`
- `QgsQncPreparedInputMappingError`

These are not the final `qnc-qgs-contract` crate. They are M2 adapter shapes
inside QGS.

## Mapping Table

| QNC-like field | QGS descriptor field | Rule |
| --- | --- | --- |
| `public_source_uri` | `identity.source_record_uri` | Public QNC URI may cross boundary. |
| `source_id` | `identity.clip_id` | Runtime-safe id, not a path. |
| `workspace_db_uri` | `identity.workspace_db_uri` | Public workspace URI only. |
| `original_media_id` | `binding.original_media_uri` | Public original media URI. |
| `proxy_media_id` | `binding.proxy_media_uri` | Public proxy media URI. |
| `private_binding` | binding booleans only | Opaque refs remain private; paths are not exposed. |
| `selected_picture_representation` | `selected_picture` | Proxy maps to `ProxyPreview`; original maps to `OriginalMedia`. |
| `authoritative_audio_representation` | `authoritative_audio` | Must be original. |
| `project_audio` | project audio fields | Sample rate must match original audio. |
| `stream_layout.original_video` | `layout.original_video` | Source timing. |
| `stream_layout.proxy_video` | `layout.proxy_video` | Required for `ProxyPreview`. |
| `audio_channels` | `audio_layout.channels` | Discrete original mono lanes. |
| `proxy_aac_authoritative` | `audio_layout.proxy_aac_authoritative` | Must remain false. |

## Validation Rules

The mapper rejects:

- missing public source URI
- missing original audio authority
- invalid source mode / selected picture mismatch
- proxy AAC as authoritative audio
- zero audio lanes
- stereo collapse / non-mono runtime audio model
- private path exposure
- invalid active range
- unknown or false timing compatibility for `ProxyPreview`
- invalid project audio
- mapped descriptor validation failure

The mapping returns structured `QgsQncPreparedInputMappingError` values and does
not panic.

## Source Identity / Private Binding Rules

Public values:

- `qnc://...` source URI
- `qnc://...` workspace URI
- public original/proxy media IDs
- runtime-safe source id

Private values:

- original/proxy path bindings
- resolver endpoints
- filesystem paths

Private values remain opaque and do not appear in public descriptor output,
snapshots, or command output.

## Media Policy Mapping

`ProxyPreview`:

- selected picture: proxy MP4
- authoritative audio: original MXF
- proxy AAC: diagnostic/fallback only

`OriginalMedia`:

- selected picture: original MXF
- authoritative audio: original MXF
- no realtime claim is added

## Audio Lane Mapping

QNC-like `AudioChannel` facts become QGS `QgsPreparedAudioChannel` lanes:

- lane index is preserved
- source track index is preserved
- source channel index is preserved
- channel kind must be `Mono`
- lane must be authoritative

No stereo collapse, L/R desktop helper, or monitor fold is accepted as runtime
audio truth.

## Project Audio Mapping

`QgsQncProjectAudioLike` carries:

- expected channel count
- expected sample rate

The mapper requires project sample rate to match original audio sample rate and
project channel count to fit the preserved original mono lanes.

## Timing / Range Mapping

The mapper carries:

- original video timebase/duration
- proxy video timebase/duration when available
- original/proxy timing compatibility
- optional active frame range

Ranges are half-open. For `ProxyPreview`, timing compatibility must be known
and true.

## qgs-test Command

New command:

```bash
cargo run -q -p qgs-test -- --qgs-qnc-prepared-input-mapping <original-mxf> <proxy-mp4>
```

The command constructs a QNC-like adapter shape from the same acceptance media
facts, maps it into `QgsPreparedInputDescriptor`, builds a `QgsInputPlan`, and
constructs `QgsBroadcastPlayerAssembly`.

The output reports:

- QNC-like public source URI
- source mode
- selected picture representation
- authoritative audio representation
- original/proxy public media IDs
- private binding presence without private paths
- original audio lane count and lane mapping
- project audio
- timing compatibility
- descriptor revision
- validation result
- descriptor / plan / assembly construction status
- no realtime, A/V sync, or device-output claims

## Sample-002 Result

Expected for Sony FX6 sample 002:

- source mode: `ProxyPreview`
- selected picture: proxy
- authoritative audio: original
- proxy AAC authoritative: no
- original audio lanes: 4
- lane identity preserved
- original/proxy timing compatible: yes
- descriptor created: yes
- input plan created: yes
- assembly construction: yes
- private path exposed: no

## Mironik 2002 Result

Mironik 2002 can use the same command when the original/proxy pair is
available. The command remains bounded to descriptor/input-plan mapping and
does not run full playback.

## Remaining Gaps

- The final `qnc-qgs-contract` crate is not implemented.
- Descriptor payload vs descriptor reference is not finalized.
- QNC DB/work-settings adapter is not implemented inside QGS.
- IPC is not implemented.
- Control-surface command/reply/snapshot/event structs remain future work.
- OriginalMedia mapping exists in the adapter, but this block does not claim
  realtime original-video playback.

## Recommended Next Block

Recommended:

```text
M2 Block R - QGS QNC Control Surface Shape Implementation
```

The descriptor bridge is now proven. The next useful step is to implement the
QNC-shaped command/reply/snapshot/event surface described by Block P, still
without IPC or QNC UI integration.

## Non-Claims

Block Q does not claim:

- shared contract crate implemented
- IPC implemented
- QNC UI integration
- realtime playback
- A/V sync
- real display output
- production audio output
- export/render

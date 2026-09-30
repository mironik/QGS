# M2 Block X - Real PreparedInput Fixture

Block X freezes two QNC-shaped prepared-input fixtures and maps them through
the existing Block Q adapter.

The fixtures are handwritten. They are not built by copying
`QgsPreparedInputDescriptor` from the Phase 22 filesystem builder.

This block does not read the QNC database, implement IPC, add a shared
contract crate, or change live playback.

## Fixtures

| Fixture | Playback input | Picture | Source mode | Active range |
| --- | --- | --- | --- | --- |
| `proxy-preview` | `ProxyIfAvailable` | proxy | `ProxyPreview` | `0..100` |
| `original-media` | `Original` | original | `OriginalMedia` | `10..90` |

Both fixtures keep:

- public `qnc://fixture/...` identity
- opaque private binding refs that stay off the mapped descriptor
- original MXF audio authority
- four discrete mono lanes
- proxy AAC non-authoritative
- 25 fps timing and 48 kHz project audio

`playback_input` must agree with source mode and selected picture:

- `Original` requires original picture and `OriginalMedia`.
- `Proxy` requires a proxy stream, proxy picture, and `ProxyPreview`.
- `ProxyIfAvailable` selects proxy picture when a proxy stream exists, and
  original picture when it does not.

A disagreement returns `PlaybackInputMismatch` and does not produce a
descriptor.

## Command

```bash
cargo run -q -p qgs-test -- --qgs-qnc-prepared-input-fixture proxy-preview
cargo run -q -p qgs-test -- --qgs-qnc-prepared-input-fixture original-media
```

The command does not take a media path.

## Non-Claims

Block X does not claim:

- a QNC database or work-settings reader
- the shared `qnc-qgs-contract` crate
- IPC or QNC UI integration
- `LoadPreparedInput` carrying this fixture yet
- realtime playback, A/V sync, real display, or production audio

Evidence remains `DescriptorMappingEvidence`.

## Block Y Follow-Up

`LoadPreparedInput` can now carry `QgsQncCommandPayload::PreparedInput`. A
successful load replaces the pre-baked plan and `Prepare` uses the fixture
active range. `Empty` remains the path used by the live command.

## Recommended Next Block

```text
M2 Block Z - Shared Contract Crate Skeleton
```

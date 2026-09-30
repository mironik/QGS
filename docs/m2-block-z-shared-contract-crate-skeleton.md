# M2 Block Z - Shared Contract Crate Skeleton

Block Z adds `crates/qnc-qgs-contract`, the draft field crate planned in
Block P.

QGS depends on it. A future QNC bridge can depend on the same crate instead
of on QGS internals. The crate does not depend on Vulkan, PipeWire, UI,
database, or serde code.

## What The Crate Owns

- contract name `qnc-qgs-contract`
- version `m2-v1-draft`
- stability `draft-internal`
- public source identity and opaque private binding
- prepared input, playback input, stream layout, and project audio
- half-open frame range and rational frame rate
- command kind and command payload, including `PreparedInput`
- the Block X proxy-preview and original-media fixtures

Validation in the crate rejects private paths, proxy AAC as authoritative
audio, stereo collapse, and playback input that disagrees with source mode.

## How QGS Uses It

`QgsQncPreparedInputLike::from_contract` copies a validated contract value
into the existing QGS adapter. The frozen fixtures are built in the contract
crate and converted at the QGS boundary. `LoadPreparedInput` still executes
inside QGS.

```bash
cargo test -q -p qnc-qgs-contract
cargo run -q -p qgs-test -- --qgs-qnc-prepared-input-fixture proxy-preview
```

## Non-Claims

Block Z does not claim:

- IPC or a process boundary
- QNC UI or database integration
- serde wire encoding
- replacement of the QGS runtime types
- realtime playback, A/V sync, real display, or production audio

Evidence level: `ContractCrateSkeletonEvidence`.

## Block AA Follow-Up

`crates/qnc-qgs-bridge` is the in-process facade. It calls this contract crate
and the QGS control surface. It does not live inside the player runtime.

## Recommended Next Block

```text
M2 Block AB - QNC host adopts qnc-qgs-bridge
```

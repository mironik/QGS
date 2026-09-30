# M2 Block Y - LoadPreparedInput Payload

Block Y lets `LoadPreparedInput` carry a QNC-shaped prepared input instead of
only an empty payload over a plan baked before the control surface exists.

This stays in-process. It does not add IPC, a shared contract crate, QNC UI,
or a database reader.

## Payload

`QgsQncCommandPayload::PreparedInput` carries `QgsQncPreparedInputLike`.

On load the control surface:

1. Rejects a private-path flag or a path-like public URI before mutation.
2. Maps the input through the Block Q adapter, including `playback_input`.
3. Rejects an active range past the selected picture duration.
4. Builds a new operational runtime from the mapped descriptor.
5. Loads that runtime only when the player is `Empty`.
6. Remembers the fixture active range for the following `Prepare`.

`Empty` remains valid when the surface was created from an existing assembly.
Frame, play, and preroll payloads are rejected on load.

A second prepared-input load is rejected with `source already loaded` and does
not replace the source already accepted.

## What Prepare Uses

After a payload load, `Prepare` uses the fixture active range. The Block X
original-media fixture therefore prepares `10..90`, not the whole duration.

Path-based live and control-session commands still send `Empty` and keep their
pre-baked assembly.

## Non-Claims

Block Y does not claim:

- QNC database or work-settings access
- IPC or QNC UI integration
- the shared `qnc-qgs-contract` crate
- realtime playback, A/V sync, real display, or production audio

## Block Z Follow-Up

`crates/qnc-qgs-contract` now holds the draft prepared-input and command
payload shapes. QGS converts those values in-process. The crate has no
backend dependencies.

## Recommended Next Block

```text
M2 Block AA - QNC-side in-process bridge prototype
```

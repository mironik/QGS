# M2 Block AB - QNC Host Adopts qnc-qgs-bridge

There is no writable QNC checkout in this workspace. Block AB therefore adds
the host seam inside `qnc-qgs-bridge` instead of editing QNC.

`QncHostPreparedRecord` is the portable extract of
`qnc_player_input::PreparedInput`: workspace URI, clip id, public media URIs,
playback input, proxy availability, project audio, original and proxy timing,
and original audio channels as sequential mono lanes. The QNC `Snapshot` stays
in QNC. QGS does not import QNC crates.

`Bridge::open_from_host` applies the QNC picture rule:

- `Original` keeps original picture.
- `ProxyIfAvailable` uses proxy picture only when a proxy exists.
- `Proxy` without a proxy returns `BridgeError::MissingProxy`.

Authoritative audio stays original MXF. Path-shaped bindings never open a
session. The mapped contract value then follows the Block AA
`LoadPreparedInput` path. There is no second player state machine.

## Check

```bash
cargo test -q -p qnc-qgs-bridge
```

## Non-Claims

Block AB does not claim a QNC controller dependency, IPC, realtime playback,
A/V sync, real display, or production audio.

Evidence level stays `InProcessBridgeEvidence`.

## Recommended Next Block

```text
M2 Block AC - command script parity
```

See `docs/m2-block-ac-command-script-parity.md`. A writable QNC checkout is
still required before the player controller can depend on this crate.

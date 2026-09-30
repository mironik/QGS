# M2 Deep Audit After Block V

This audit rechecks the QGS Broadcast Player / QNC backend path after Blocks
R through V, including the uncommitted Block V V2 cleanup in `qgs-test` and
the Block V document.

It is a stabilization and honesty audit. It does not implement runtime
features, IPC, QNC UI, Wayland/Vulkan, production audio, realtime playback,
A/V sync certification, or export/render.

Baseline:

- HEAD `a1cea2e` — live preview video and original audio output
- Working tree: Block V V2 (defaults `none`/`none`, desktop monitor mode,
  compact output labels)
- `cargo test -p qgs-media-runtime`: 177 passed
- `cargo check -p qgs-test`: pass

## Score

Overall score for the M2 Broadcast Player as a QNC backend path: **6.8 / 10**.

This is a strong control and diagnostic player, not a production air path and
not a finished QNC bridge.

| Area | Score | Meaning |
| --- | --- | --- |
| QGS/QNC ownership and media policy | 8.5 | No QNC crate imports. Original MXF audio, discrete mono lanes, proxy AAC non-authoritative. |
| Control surface (Block R) | 8.0 | In-process envelopes, generation checks, public snapshots. Load still has no descriptor payload. |
| Live loop and stdin (Blocks T/U) | 7.5 | Same control surface, honest logical vs wall pace, non-claims preserved. |
| Live output honesty (Block V) | 6.5 | V2 defaults and diagnostic labels are better. Uncommitted seek/audio cursor can lie. |
| Verification matrix discipline | 6.0 | Block R is in the code matrix. Blocks T/U/V are not. |
| QNC bridge completeness | 4.0 | Plan plus synthetic mapping. No real QNC `PreparedInput`, shared crate, or IPC. |
| Production display / realtime / A-V | 2.0 | Intentionally not implemented. Non-claims stay false. |

## What Changed Since The Post-Block-Q Audit

| Earlier finding | Now |
| --- | --- |
| Verification matrix missing Block M–Q levels | Closed through Block R in code and Step 20Q doc. |
| Uncommitted control-surface compile failure | Closed. Committed Block R compiles; tests pass. |
| `FramePresented` easy to over-read | Improved. Live summary explicitly says real-display claim is no. |
| Stale `docs/safety.md` counts | Improved. Inventory was updated with later Vulkan work. |
| Block Q round-trip is not real QNC input | Still open. |
| `playback_input` unused in mapping | Still open. |
| `LoadPreparedInput` has empty payload | Still open. |
| Assembly is a thin factory | Still true, and now documented as such. |
| Matrix drift | Reopened for Blocks T, U, and V. |

## Executive Summary

Blocks R–V turned the player from a scripted proof into an operator-facing live
loop:

```text
QgsBroadcastPlayerAssembly
  -> QgsQncControlSurface
  -> QgsBroadcastPlayerOperationalRuntime
```

Stdin pause, play, seek, status, stop, and quit go through that surface with
generation checks. Default live output is quiet. Preview PPM files and
PipeWire desktop monitoring are explicit opt-ins and are labeled diagnostic.

The product is still not ready to replace the QNC broadcast engine. QNC does
not yet send a real `PreparedInput`. QGS still builds descriptors from local
MXF/MP4 paths inside `qgs-test`. There is no shared contract crate and no IPC.
Real display, production routing, realtime, and A/V sync remain
`NotImplemented` / false, and the code mostly says so.

## Findings

| Severity | Location | Finding |
| --- | --- | --- |
| High | `tools/qgs-test/src/main.rs` live audio `emit` | Uncommitted V2 keeps `covered_until_sample`. If the playhead sample is behind that cursor, emit returns `playing` or `pending` and does not submit PCM. Seek backward, or a seek that stays behind the cursor, can show `audio=playing` with silence. Seek paths do not reset the cursor. |
| High | code matrix vs Blocks T/U/V docs | `BroadcastRuntimeVerifiedSubsystem` and Step 20Q stop at the Block R control surface. Live loop, stdin control, and live preview/audio have no matrix rows, so the canonical truth table under-reports shipped behavior. |
| Medium | `qgs-media-runtime` mapping | `playback_input` is stored and never read by `map_to_qgs_descriptor`. Picture mode follows `source_mode` and selected representation only. |
| Medium | `qgs-test` assembly builders | Acceptance still synthesizes `qnc://` identity from filesystem paths. That proves QGS mapping, not QNC DB/work-settings consumption. |
| Medium | Block R command payload | `LoadPreparedInput` is `Empty`. The plan is baked into the assembly before the control surface exists. |
| Medium | Block V compact labels | `audio=playing` reads like continuous playback. Submission is chunked (desktop monitor every 25 frames) and not a realtime scheduler. |
| Low | Block V document | “Both modes” vs three audio modes, and duplicated `pipewire-4mono` wording. |
| Low | Block R document footer | Still recommends Block S as next, although S–V already exist. |

No critical false certification was found. Live summaries keep real display,
visual verification, realtime, production audio, A/V sync, and real
`FramePresented` at no.

## What Is Solid

1. Ownership split: QNC stays application/DB/UI in the plans; QGS does not
   import QNC crates.
2. Original MXF audio authority and discrete mono lanes on descriptor, plan,
   assembly, and live output. Proxy AAC is not the live audio source.
3. Desktop monitor (tracks 4 then 1) is explicitly a listening helper, matching
   the earlier Mironik diagnostic, not broadcast routing.
4. `pipewire-4mono` still submits tracks 1–4 as channels 1–4, with a short
   chunk cap, without claiming physical mapping.
5. Block R generation: stale commands reject without mutation; snapshots do not
   advance generation.
6. Live mutations go through `QgsQncControlSurface`, not a second state machine.
7. V2 default `--video-output none --audio-output none` stops accidental
   diagnostic spam.
8. Preview files are real proxy decode/readback artifacts, periodic, and marked
   `real_display=no` / `visual_verified=false`.
9. Runtime tests are green (177). `qgs-test` checks clean.

## Continuation Plan

Do not start Wayland/Vulkan or a QNC UI bridge as the next block. Close the
live-output bug and the matrix gap first, then the input contract QNC will
actually send.

### Block W — Live output correctness

- Reset `covered_until_sample` on seek, stop, and source reload.
- Do not report `playing` when the current range was not submitted.
- Add a test: seek backward, then the next emit submits from the new range.
- Add matrix rows for live loop (T), stdin control (U), and diagnostic live
  output (V), below `AudioDeviceVerified`, `VisualVerified`, and
  `RealtimeVerified`.

### Block X — Real PreparedInput fixture

- Freeze one ProxyPreview and one OriginalMedia fixture that is not built by
  copying the Phase 22 descriptor.
- Map it through Block Q.
- Validate `playback_input`, or remove it from the public adapter until it is
  real.
- Keep private paths out of public snapshots.

### Block Y — Load payload on the control surface

- `LoadPreparedInput` carries a descriptor or a stable descriptor reference.
- Assembly/session consumes that payload instead of a pre-baked plan only.
- Still in-process. No IPC.

### Block Z — Shared contract crate skeleton

- Extract the Block P / Block R field set into `qnc-qgs-contract`.
- QGS and a future QNC bridge depend on that crate, not on each other's
  internals.
- No Vulkan, PipeWire, or UI dependencies in the crate.

### After Z — QNC-side bridge prototype

Follow the Block S order: in-process adapter, command script parity, passive
view projection mocks, then IPC.

### Later — display window

A minimal Wayland + Vulkan preview window is the right display step, but only
after W–Y. X11 stays a non-target for QNC OS.

## Non-Claims

This audit does not claim production display, production audio, realtime
playback, A/V sync, QNC UI integration, a shared contract crate, or completion
of the QNC engine replacement.

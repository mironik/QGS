# M2 Block W - Live Output Correctness

Block W corrects the live audio cursor and records Blocks T, U, and V in the
verification matrix.

It does not add Wayland/Vulkan, IPC, QNC UI, a shared contract crate, realtime
certification, A/V sync certification, or production audio routing.

## Why This Block Exists

Block V V2 kept a monotonic `covered_until_sample` cursor. After a seek to an
earlier sample, the next emit skipped PipeWire submission and could still
report `playing`. A successful desktop-monitor chunk used the same `playing`
label, which reads like continuous playback.

## Audio Coverage Cursor

`QgsLiveAudioCoverage` records the last submitted original-MXF sample range.

- A new live output starts uncovered.
- A successful submit marks `start + count` as covered.
- Ticks whose playhead is still inside that range return `covered`, not
  `playing`.
- An accepted seek resets the cursor before the next emit.
- An accepted stop resets the cursor.
- Cumulative submit counts stay for the run summary. Reset clears only the
  cursor.

After a backward seek, `next_submit_start` returns the new range, so the next
emit is eligible to submit from that range.

Desktop-monitor success is labeled `submitted-monitor`. Discrete 4-mono success
stays `submitted-4mono`.

## Verification Matrix

The code matrix and Step 20Q now include:

| Subsystem | Level |
| --- | --- |
| broadcast player live runtime loop | `LiveRuntimeLoopEvidence` |
| broadcast player live control input | `LiveControlInputEvidence` |
| broadcast player live diagnostic output | `LiveDiagnosticOutputEvidence` |

Truth rules keep those levels below `VisualVerified`, `AudioDeviceVerified`,
and `RealtimeVerified`. Real display, real speaker output, and realtime
playback stay `NotImplemented`.

## Non-Claims

Block W does not claim:

- real display output
- visual verification
- production PipeWire routing
- `AudioDeviceVerified`
- realtime playback
- A/V sync
- QNC UI or IPC

## Block X Follow-Up

Block X freezes ProxyPreview and OriginalMedia prepared-input fixtures that are
not synthesized from the Phase 22 descriptor, and rejects `playback_input`
values that disagree with source mode and selected picture.

## Recommended Next Block

```text
M2 Block Y - LoadPreparedInput Payload
```

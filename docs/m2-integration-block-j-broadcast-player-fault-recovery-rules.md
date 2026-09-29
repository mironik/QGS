# M2 Integration Block J — Broadcast Player Fault and Recovery Rules

Block J adds production-shaped fault and recovery rules to the QGS Broadcast
Player Operational Runtime.

This block is not a display/audio experiment. It does not implement
Wayland/Vulkan, X11, DRM/KMS, realtime playback, A/V sync, QNC UI integration,
export/render, screenshot/readback diagnostics, or new media decode paths.

## Why This Exists

Block I proved deterministic operational behavior on a mostly successful
runtime path. A production Broadcast Player backend also needs to say what
happens when commands are illegal, readiness is missing, the cue/seek target is
outside the active range, prepared state is empty or underrun, or production
device backends are unavailable.

Block J makes those cases machine-readable so future QNC applications can issue
commands, observe events/snapshots, and decide what to do without owning QGS
backend readiness or media timing policy.

## Fault Model

Faults are represented as structured QGS runtime facts. Each fault carries:

- kind
- severity
- scope
- recoverable flag
- recommended recovery action
- recovery status
- QNC-safe code
- public-safe reason

The main fault kinds now include:

- `IllegalCommandForState`
- `SourceNotLoaded`
- `SourceAlreadyLoaded`
- `StaleSourceHandle`
- `StaleGeneration`
- `CueOutsideActiveRange`
- `SeekOutsideActiveRange`
- `PrepareRequired`
- `PrepareFailed`
- `PreparedWindowUnderrun`
- `PreparedWindowEmpty`
- `EndOfRangeReached`
- `BackendNotImplemented`
- `RealDisplayUnavailable`
- `AudioOutputNotProductionVerified`
- `VisualVerificationUnavailable`
- `RealtimeVerificationUnavailable`
- `AvSyncNotVerified`
- `PrivatePathExposureBlocked`
- `UnsupportedQncOsBackend`
- `ProxyAudioNotAuthoritative`

No unprobed hardware failures are invented.

## Severity Model

Severity values are:

- `Info`
- `Warning`
- `Recoverable`
- `Fatal`

The tested Block J scenario contains recoverable command/timing faults and
non-fatal backend/device warnings. Fatal count remains zero.

## Fault Scopes

Fault scopes are:

- `Command`
- `Source`
- `Prepare`
- `Playback`
- `Buffer`
- `Device`
- `Timing`
- `Session`

Scopes let observers separate a bad command from a device-policy warning or a
timing/range issue.

## Recovery Actions

Recovery actions are:

- `NoActionRequired`
- `RetryCommandAfterPrepare`
- `PrepareAgain`
- `ReCue`
- `SeekToValidRange`
- `StopThenUnload`
- `UnloadAndReload`
- `SelectDifferentDevicePolicy`
- `UseDiagnosticBackendOnly`
- `WaitForBackendImplementation`
- `FatalRequiresNewSession`

These actions are suggestions unless a future recovery command explicitly
executes them. QGS does not silently auto-recover from rejected commands.

## State Mutation Rules

Rejected commands do not mutate operational transport state. The fault
transcript is updated, but status, current frame, source-loaded state, and
active range remain unchanged.

Current rules:

- illegal command while `Empty`: rejected, no state mutation
- source not loaded: rejected, recovery suggests loading/reloading
- play before `Ready`: rejected, recovery suggests prepare again
- cue outside active range: rejected, cue remains unchanged
- seek outside active range: rejected, current position remains unchanged
- backend not implemented: warning visible, runtime does not enter `Failed`
- audio production not verified: warning visible, runtime does not enter
  `Failed`
- unload clears source state but preserves the session fault transcript

## Backend And Device Warnings

The snapshot reports backend/device warnings for:

- real display backend not implemented
- real display unavailable
- visual verification unavailable
- realtime verification unavailable
- A/V sync not verified
- production audio output not verified

These warnings are not fatal in the current runtime. Recommended actions are
either `WaitForBackendImplementation` or `UseDiagnosticBackendOnly`.

## Prepared Window Policy

Prepared-window faults are recoverable:

- `PreparedWindowEmpty` means a source is loaded but no prepared frames are
  available.
- `PreparedWindowUnderrun` means the selected/current frame is outside the
  prepared window.

The recovery action is `PrepareAgain`. QGS does not fake payload readiness.

## Stale Source / Generation Policy

Stale source handles and stale generations are represented as recoverable
session/source faults with `UnloadAndReload` style recovery. Existing Phase 26
and Phase 28 paths still reject stale commands without mutation.

## Private Path Policy

Public command, event, snapshot, and fault output must not expose raw private
filesystem paths. If private path exposure is attempted, QGS blocks it and can
record `PrivatePathExposureBlocked` without including the private value.

## Snapshot Fields

`QgsOperationalRuntimeSnapshot` now includes:

- active faults
- `QgsBroadcastPlayerFaultSnapshot`
- last fault
- fault counters
- recoverable fault count
- fatal fault count
- recommended recovery action
- backend/device warnings
- private path exposure flag

The legacy `faults` list remains available for compatibility with Block I.

## Events

Block J adds fault/recovery-oriented event kinds:

- `PlayerCommandRejected`
- `PlayerFaultRecorded`
- `PlayerRecoverySuggested`
- `PlayerEnteredFailed`
- `PlayerRecovered`
- `PlayerWarningRecorded`

The current runtime emits fault recorded, recovery suggested, warning recorded,
and command rejected events for the tested rejected-command path. It does not
emit recovered unless a recovery action is actually executed.

## qgs-test Command

Fault/recovery acceptance is available through:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-fault-recovery <original-mxf> <proxy-mp4>
```

The command runs this deterministic scenario:

1. `Play` while `Empty`
2. `Seek` while `Empty`
3. `LoadPreparedInput`
4. `Cue` outside active range
5. `Prepare [0..1000 ms)`
6. `Play` before ready
7. `Cue frame 0`
8. `Tick`
9. `Play`
10. `Seek` outside active range while playing
11. two playing ticks
12. `Stop`
13. `Unload`
14. stale/invalid `Seek` after unload

The report prints command counts, fault counts, last fault, severity, scope,
recoverability, recommended recovery action, rejected-state mutation status,
source-loaded status after rejected commands, final snapshot, and the standard
non-claims.

## Sony FX6 Sample 002 Result

Observed result:

- command count: 15
- accepted commands: 9
- rejected commands: 6
- faults recorded: 11
- recoverable fault count: 11
- fatal fault count: 0
- last fault: `SeekOutsideActiveRange`
- last severity: `Recoverable`
- last scope: `Timing`
- recommended recovery: `SeekToValidRange`
- rejected command state mutation count: 0
- source loaded after rejected commands: `no, no, yes, yes, yes, no`
- status sequence: `Empty -> Empty -> Loaded -> Loaded -> Loaded -> Loaded -> Ready -> Ready -> Playing -> Playing -> Playing -> Playing -> Stopped -> Empty -> Empty`
- private path exposed: no
- real display: `NotImplemented`
- visual verified: no
- realtime verified: no
- audio device production verified: no
- A/V sync verified: no
- X11 target: no / legacy non-target
- real backend `FramePresented`: no

## Mironik 2002 Result

The Mironik 2002 run matched the sample 002 behavior:

- command count: 15
- accepted commands: 9
- rejected commands: 6
- faults recorded: 11
- recoverable fault count: 11
- fatal fault count: 0
- last fault: `SeekOutsideActiveRange`
- recommended recovery: `SeekToValidRange`
- rejected command state mutation count: 0
- private path exposed: no
- real display, visual verification, realtime verification, production audio
  verification, and A/V sync remain false

## Media And Device Policy

Block J preserves the current QGS media policy:

- proxy MP4 video is responsive preview/edit picture
- original MXF audio is authoritative
- original MXF video remains `OriginalMedia` / finishing mode
- proxy MP4 AAC is diagnostic/fallback only, never authoritative
- broadcast/news audio remains discrete mono-channel based
- no stereo collapse

It also preserves the display/device policy:

- QNC OS real display target is Wayland + Vulkan
- X11 is legacy/non-target only
- DRM/KMS + Vulkan remains optional future direct/appliance path
- PipeWire prototypes are not production `AudioDeviceVerified`

## Verification Matrix

Step 20Q adds:

- subsystem: `broadcast player fault and recovery rules`
- evidence level: `FaultRecoveryPolicyEvidence`

This proves structured fault/recovery policy evidence only. It does not upgrade
real display output, real backend `FramePresented`, `VisualVerified`,
`RealtimeVerified`, `AudioDeviceVerified`, or A/V sync.

## Non-Claims

Block J does not claim:

- real display output
- real backend `FramePresented`
- visual verification
- realtime playback
- A/V sync
- production PipeWire audio output
- `AudioDeviceVerified`
- QNC UI integration
- export/render
- Wayland/Vulkan implementation
- X11 implementation
- DRM/KMS implementation

## Next Recommended Block

M2 Integration Block K — Broadcast Player Contract Freeze / QNC API Surface.

This should freeze the command/snapshot/event/fault surface enough for future
QNC-facing adapter work, while still avoiding real display/audio backend
implementation until the runtime contract is stable.

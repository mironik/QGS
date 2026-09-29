# M2 Runtime Lego Module Architecture Stabilization

This stabilization audit records the module boundaries that emerged from M2
Step 22A through Integration Block C.

It is not a new runtime feature. It does not add IPC, QNC UI integration, real
display output, realtime playback, A/V sync, production audio policy,
export/render, or new decode behavior.

## Why This Audit Exists

Integration Block C proved that the current QGS runtime surfaces can be wired
together:

```text
Phase 22 InputPlan
  -> session command/runtime control
  -> transport readiness
  -> bounded tick preparation
  -> presenter boundary
  -> monitor projection
  -> passive view and event transcript
```

That success creates a risk: the in-process acceptance path can start to look
like one large runtime object that owns every concern. That would be the wrong
shape for the future QGS/QNC OS Broadcast Player.

The target architecture is a set of narrow Lego modules. Each module owns one
kind of backend fact or boundary. The session runtime composes them, but it
does not absorb their semantics.

## Monolith Risk After Blocks A And C

The current `qgs-media-runtime` crate intentionally contains many backend-neutral
types while the runtime surface is still stabilizing. That is acceptable for
M2, but it must not become the architectural model.

Risks to avoid:

- making `QgsSessionRuntime` the owner of media decode rules
- making command execution own frame-clock semantics
- making `TickPrepare` imply realtime playback
- letting monitor projection imply real display presentation
- merging presenter evidence with visual verification
- merging PipeWire diagnostics with production audio policy
- exposing private local paths as public command, event, or view identity
- adding a second input model beside Phase 22 `QgsInputPlan`

## Target Module Boundaries

| Module | Owns | Does Not Own |
| --- | --- | --- |
| `input_descriptor` | QNC-compatible prepared input descriptor, source roles, public URI identity, private binding checks. | Transport state, decode, presenter evidence, QNC DB reads. |
| `input_plan` | Conversion from descriptor to QGS session plan, source mode, original/proxy timing facts, original audio lane plan. | Command queue, playback state, device output. |
| `transport` | Source handles, load/preload/set-active, active range, cue, prepared anchor, play-ready, no-work-on-Play rules. | Input discovery, decode, presenter submission, realtime scheduling. |
| `frame_clock` | Rational frame timing, active range timing, due-frame calculations, frame/sample mapping facts. | UI/export FPS policy, device clock ownership, decode. |
| `prepared_buffer` | Bounded prepared frame/audio-range records and tick preparation accounting. | Real display/audio submission, realtime loop, visual/audio verification. |
| `lifecycle_events` | Internal runtime event envelope, generation, close/unload discard semantics. | QNC IPC, UI state, media decode. |
| `qnc_projection` | QNC-shaped command/event/passive-view projection with private-path filtering. | QNC crate imports, process protocol, UI rendering. |
| `command_executor` | Generation-checked application of QNC-shaped command envelopes to existing transport methods. | Command queue policy, IPC, media decode. |
| `session_control` | Bounded FIFO queue, duplicate command-id policy, transcript, passive snapshots. | Media timing semantics, presenter semantics, device policy. |
| `presenter_boundary` | Prepared video payload descriptor, test presenter submission, presenter evidence, monitor projection facts. | Real display backend, visual verification, session command ownership. |
| `verification_matrix` | Evidence-level truth table and non-overclaiming rules. | Runtime behavior. |
| `audio_pipewire_boundary` | Linux/PipeWire diagnostic device boundary and native submission/drain evidence. | Backend-neutral media truth, QNC UI, production audio routing policy. |

## Dependency Rules

Allowed:

- `session_control` may depend on the command executor and passive projection.
- `command_executor` may call existing transport, lifecycle, and prepared-buffer
  methods.
- `presenter_boundary` may consume prepared video payload descriptors and
  produce monitor projection updates.
- `verification_matrix` may refer to all subsystems as evidence labels.
- Linux/PipeWire code may depend on PipeWire-specific crates in an isolated
  Linux/device-boundary crate.

Forbidden:

- QGS runtime crates must not import QNC crates.
- Backend-neutral runtime types must not depend on PipeWire, Wayland, X11,
  DRM/KMS, QNC UI, or database crates.
- Presenter evidence must not depend on the session command queue.
- Input planning must not depend on qgs-test raw filesystem path output.
- `Play` must not open media, decode, fill queues, or build first usable
  working state.
- Proxy AAC must not become authoritative audio.
- Stereo/desktop monitor helpers must not replace discrete original mono audio
  lane identity.

## Phase Mapping

| Milestone | Lego Module |
| --- | --- |
| Step 22A | Replacement map and ownership baseline. |
| Phase 22 | `input_descriptor` and `input_plan`. |
| Phase 23 | `transport`. |
| Phase 24 | `frame_clock` and active range/cue timing. |
| Phase 25 | `prepared_buffer` and tick preparation accounting. |
| Phase 26 | `lifecycle_events`. |
| Phase 27 | `qnc_projection`. |
| Phase 28 | `command_executor`. |
| Phase 28S | Stabilization audit for Phase 22-28 surfaces. |
| Integration Block A | `session_control` orchestration around Phase 28. |
| Integration Block B | `presenter_boundary` and monitor projection. |
| Integration Block C | End-to-end wiring acceptance across existing modules. |

## Current Acceptable Couplings

These couplings are acceptable during M2 stabilization:

- `qgs-media-runtime/src/lib.rs` contains several backend-neutral modules in one
  file while the API shape is still moving.
- `QgsQncSessionCommandExecutor` directly calls transport and prepared-buffer
  methods because Phase 28 is an in-process executor, not an IPC adapter.
- `QgsSessionRuntime` owns a concrete `QgsQncSessionCommandExecutor` so Block A
  can prove queue and transcript behavior without inventing a second command
  model.
- `qgs-test` composes media inspection, input planning, session control, and
  presenter-boundary helpers for acceptance commands.
- Presenter test-boundary evidence is projected into monitor facts, as long as
  it remains clearly below real display evidence and visual verification.

## Couplings To Split Later

Future cleanup should split code along these logical lines once APIs settle:

- move Phase 22 descriptor/plan types into an input module
- move transport source/range/cue/anchor state into a transport module
- move frame-clock and sample mapping helpers into a timing module
- move prepared-buffer/tick-preparation records into a prepared-state module
- move command projection and passive views into a projection module
- move command executor/session queue into a session-control module
- move presenter descriptors/evidence/monitor projection into a presenter module
- keep PipeWire/Linux device code outside backend-neutral runtime crates

This split should be done as a low-risk mechanical cleanup after the current
surface has fewer API changes. It should not change behavior.

## Command Inventory

The current Lego surface is covered by these qgs-test commands:

| Command | Module Checked |
| --- | --- |
| `--qnc-prepared-input-descriptor` | Phase 22 descriptor. |
| `--qgs-input-plan` | Phase 22 input plan. |
| `--qgs-transport-engine-parity` | Phase 23 transport skeleton. |
| `--qgs-frame-clock-parity` | Phase 24 timing/cue parity. |
| `--qgs-playout-buffer-tick` | Phase 25 prepared buffer and tick preparation. |
| `--qgs-runtime-lifecycle-events` | Phase 26 lifecycle event envelope. |
| `--qgs-qnc-event-projection` | Phase 27 QNC-shaped projection. |
| `--qgs-session-command-boundary` | Phase 28 command executor. |
| `--qgs-session-runtime-control` | Integration Block A session orchestration. |
| `--qgs-presenter-monitor-boundary` | Integration Block B presenter/monitor boundary. |
| `--qgs-runtime-surface-e2e` | Integration Block C end-to-end wiring. |

## What Block C Proves

Block C proves that the Lego modules can be connected into one deterministic
backend acceptance scenario without leaking private paths or claiming real
output.

It does not prove:

- realtime playback
- A/V sync
- real display output
- visual verification
- production audio output
- full audio-device verification
- QNC UI integration
- IPC or process-launcher behavior

## Presenter Backend Follow-Up

M2 Integration Block D is the real display presenter backend audit. It preserves
the same Lego shape and recommends a staged path: first a screenshot/file
presenter visual diagnostic, then a minimal Wayland/Vulkan presenter prototype.

It should preserve the Lego shape:

- presenter backend evidence stays in the presenter boundary
- monitor projection remains passive
- session control remains orchestration
- visual verification remains separate from successful presenter submission
- no realtime or QNC UI claim is made until evidence supports it

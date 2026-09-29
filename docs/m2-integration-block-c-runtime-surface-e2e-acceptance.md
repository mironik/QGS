# M2 Integration Block C - Runtime Surface End-to-End Acceptance

Integration Block C proves that Integration Block A and Integration Block B work
together as one coherent QGS backend session scenario.

This is an acceptance block. It does not add a new subsystem, IPC, QNC UI, real
display output, realtime playback, A/V sync, audio output policy, or
export/render.

It also does not justify collapsing the runtime into a monolith. Block C is a
wiring proof for narrow Lego modules: session control remains orchestration,
transport/prepared-buffer semantics remain in their existing modules, and the
presenter/monitor boundary remains independent from command execution.

The accepted path is:

```text
Phase 22 InputPlan
  -> Integration Block A session runtime command queue
  -> transport readiness
  -> Phase 25 tick preparation
  -> Integration Block B presenter boundary submission
  -> monitor projection update
  -> final passive view / event transcript
```

## Relationship To Block A

Block A added the in-process session runtime control surface:

- bounded command queue
- strict command id handling
- generation checks
- FIFO execution
- passive view snapshots
- public command transcript

Block C uses the same command envelopes and executor. It does not create a
second command model and does not implement IPC.

## Relationship To Block B

Block B added the presenter/monitor boundary:

- prepared video payload descriptor
- test presenter submission
- explicit test presenter evidence
- monitor projection update
- conservative real-display and visual-verification flags

Block C uses that boundary after the session runtime reaches prepared-buffer
state through `TickPrepare`.

The presenter boundary remains an independent device/presenter-facing edge. It
is not owned by the session control surface, and test-presenter evidence is not
promoted into real display evidence.

## End-To-End Command

```bash
cargo run -q -p qgs-test -- \
  --qgs-runtime-surface-e2e <original-mxf> <proxy-mp4>
```

The command executes:

1. `LoadPreparedInput`
2. `PreloadSource`
3. `SetActiveSource`
4. `SetActiveRange [0..1000 ms)`
5. `Cue frame 0`
6. `PrepareAnchor`
7. `Play`
8. `TickPrepare`
9. presenter boundary submission for selected prepared frame 0
10. monitor projection update
11. `Pause`
12. `CloseActiveSource`

The presenter boundary uses the existing proxy-preview video payload path:
proxy MP4 decode, VA -> CPU NV12 transfer, Vulkan processing, and processed GPU
frame token. The presenter is `TestPresenter`.

## Acceptance Results

For Sony FX6 sample 002 / Mironik 1560:

- public original URI: `qnc://local/media/original/Mironik-1560`
- public proxy URI: `qnc://local/media/proxy/Mironik-1560`
- private path exposed: no
- command queue accepted count: 10
- command queue rejected count: 0
- transport `play_ready` before Play: yes
- prepared buffer count after `TickPrepare`: 6
- selected prepared frame: 0
- presenter boundary submitted: yes
- presenter evidence kind: `TestPresenterAccepted`
- test presenter accepted: yes
- monitor projection prepared descriptor: yes
- monitor projection submitted: yes
- real display evidence: none
- visual verified: no
- real presented: no
- event sequence monotonic: yes
- final state after cleanup: `Loaded`, active source none, `play_ready=false`

For Mironik 2002:

- public original URI: `qnc://local/media/original/Mironik-2002`
- public proxy URI: `qnc://local/media/proxy/Mironik-2002`
- same end-to-end result over selected prepared frame 0
- private path exposed: no
- no realtime/display/A-V sync/visual verification claim

The command remains large-file safe for the presenter path. It does not slurp
the original MXF payload to prove video presenter/monitor behavior.

## Evidence Distinctions

Block C keeps the important boundaries intact:

- End-to-end wiring does not mean the modules should be merged.
- Session control coordinates modules; it does not own media decode, timing, or
  presenter evidence semantics.
- The presenter boundary remains separately testable.
- Prepared does not mean SubmittedToPresenter.
- SubmittedToPresenter does not mean real display Presented.
- `TestPresenterAccepted` does not mean real display output.
- Monitor projection can show submitted/test evidence.
- Monitor projection does not show real display presentation.
- `VisualVerified` remains no.
- `RealtimeVerified` remains no.
- `AudioDeviceVerified` remains no.

The Step 20Q verification matrix records this block as `TestBoundaryEvidence`.

## Non-Claims

Not implemented:

- IPC
- QNC UI
- real display output
- Wayland/X11/DRM/KMS/swapchain presenter
- realtime playback
- A/V sync
- production audio output policy
- visual verification
- export/render

## Next Recommended Block

M2 Integration Block D - Real Display Presenter Backend Audit.

That block should inspect candidate real presenter backends and define the
evidence needed to move from test presenter evidence toward real display
presentation. It should still keep `VisualVerified` separate from successful
submission to a display backend.

# M2 Block U - Broadcast Player Live Control Input

Block U adds stdin operator controls to the existing QGS Broadcast Player live
runtime command:

```bash
--qgs-broadcast-player-live <original-mxf> <proxy-mp4>
```

This is not IPC, not QNC UI, not a process launcher, not a realtime
certification block, and not a device backend. It keeps using the existing
runtime chain:

```text
QgsBroadcastPlayerAssembly
  -> QgsQncControlSurface
  -> QgsBroadcastPlayerOperationalRuntime
```

## Purpose

Block T made the backend player run as a live logical loop. Block U makes that
loop controllable by an operator at the terminal without adding a second input
model or a second player state machine.

The control surface still owns command legality, generation checks, snapshots,
events, and private-path filtering.

## Interactive Commands

Interactive controls are enabled by default. Use `--no-interactive` to preserve
the Block T non-interactive behavior.

| Input | Meaning |
| --- | --- |
| `p`, `pause` | Pause the live loop and stop requesting one-frame play ticks. |
| `r`, `resume`, `play` | Resume logical ticking through `Play`. |
| `s <frame>`, `seek <frame>` | Seek to a source frame, refresh preroll, and resume only if the loop was previously ticking. |
| `status` | Request a snapshot through the control surface without mutating generation. |
| `stop` | Stop the runtime and keep the terminal control loop available. |
| `q`, `quit` | Exit the live loop and run normal stop/unload cleanup. |
| `help` | Print the available stdin controls. |

Invalid commands are reported and ignored.

## Operator Flow

Example:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <original-mxf> <proxy-mp4> \
  --status-every 25 --view compact
```

Example controls:

```text
status
pause
status
seek 250
status
play
status
quit
```

The compact view prints operator-facing status lines and command replies. The
detailed view preserves the richer command envelope output.

If a paused seek exposes an existing prepared-buffer window that does not cover
the new frame yet, compact output reports `covers_current=no` and labels the
buffer as `stale` instead of claiming that the old window is ready for the new
position. That is a runtime preparation policy limitation, not a new playback
claim.

## Non-Interactive Compatibility

This command preserves the Block T mode:

```bash
cargo run -q -p qgs-test -- --qgs-broadcast-player-live \
  <original-mxf> <proxy-mp4> \
  --max-frames 80 --status-every 10 --view compact --no-interactive
```

In non-interactive logical mode, the loop advances as fast as command execution
allows. Interactive logical mode sleeps briefly between ticks so stdin commands
can be observed without claiming realtime playback.

## Boundaries Preserved

Block U does not implement:

- QNC UI integration
- IPC
- Wayland/Vulkan display presentation
- X11, DRM/KMS, or swapchain presentation
- production PipeWire audio
- realtime playback verification
- A/V sync certification
- export/render

Truth boundaries remain:

- real display: `NotImplemented`
- QNC OS display target: Wayland + Vulkan
- X11: legacy/non-target only
- visual verified: no
- realtime verified: no
- production audio verified: no
- A/V sync verified: no
- real `FramePresented`: no
- proxy MP4 video is the ProxyPreview picture source
- original MXF audio is authoritative
- proxy AAC is non-authoritative
- original MXF mono lanes remain discrete

## Implementation Notes

The stdin reader is a small helper thread that sends lines over a standard
library channel. `qgs-test` parses those lines, but all accepted runtime changes
go through `QgsQncControlSurface`.

`status` uses a `Snapshot` command and is non-mutating. Pause/resume/seek/stop
use the same QNC-shaped command envelopes as the rest of the live runtime.

No private filesystem paths are printed in the public command/event/view output.

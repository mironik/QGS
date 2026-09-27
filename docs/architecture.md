# QGS Architecture

QGS is an independent, vendor-neutral media and GPU system service. It is
independent from Qnc: Qnc may eventually become one client, but QGS must not
contain Qnc-specific concepts.

QGS is intended for any media application that needs a system service for
discovering and coordinating media/GPU capabilities.

QGS reports hardware capabilities. Application schedulers make workload
decisions. The service should provide accurate system information and control
interfaces without deciding how an application should partition its work.

The control plane and media data plane are separate. Normal protocol messages
are for control, discovery, negotiation, and coordination.

Large video/GPU data must not travel through normal protocol messages. Bulk
media and GPU resources need mechanisms designed for large data movement and
sharing.

Future zero-copy interoperability will use native mechanisms such as DMA-BUF
and Vulkan external memory where appropriate.

QGS must not attempt to replace Vulkan. Vulkan remains the rendering and GPU
execution API; QGS should coordinate capabilities and integration points around
such APIs.

QGS should eventually be Linux-distribution independent. Linux-specific
integration can exist in future platform layers, but the protocol model should
not require a specific distribution.

## Device Discovery Boundary

The M1 device-discovery path is:

```text
Application
    |
QGS Protocol
    |
qgsd / qgs-core
    |
DeviceDiscovery abstraction
    |
qgs-vulkan
    |
Vulkan loader/driver
    |
hardware
```

Vulkan-specific types and handles stop at `qgs-vulkan`. `qgs-core` and
`qgs-protocol` use QGS-owned device descriptions and must not expose
`VkPhysicalDevice` or other Vulkan implementation details.

## Resource Ownership Boundary

The M1 buffer-resource path is:

```text
Session
    owns
ResourceId
    represents
Buffer
    backed by
backend resource
```

`qgs-core` owns resource lifecycle semantics. A `ResourceId` is non-zero,
opaque, session-owned, and not persistent across daemon restarts. A resource
must not outlive the session that owns it, and a resource from one session is
not usable by another session. When a client disconnects, its session is
dropped and all resources still registered under that session are released by
Rust ownership/drop.

`qgs-vulkan` owns the Vulkan implementation objects that back buffers.
Vulkan buffers, memory objects, and handles do not escape `qgs-vulkan`.
`qgs-protocol` owns only the protocol representation.

M1 Step 5 implements buffers only. The maximum single buffer size is 64 MiB as
a conservative M1 safety limit, not as a final product limit.

M1 Step 6 adds Linux external-memory sharing for buffers that were created as
exportable. Exporting a resource does not transfer QGS ownership: the
`ResourceId` remains owned by its session, while the exported native FD is a
duplicated OS/Vulkan reference transferred to the client. Client-side imported
objects have their own lifetime after import according to OS and Vulkan
external-memory semantics. QGS does not add distributed reference counting.

The control/data/native-handle split is:

```text
QGS protocol payload metadata
    !=
native Linux transport attachment
    !=
shared GPU resource contents
```

Normal QGS protocol messages carry bounded metadata only. The Linux transport
uses `SCM_RIGHTS` for the native FD attachment. Buffer contents are not copied
through IPC.

Unsafe Rust remains prohibited in all normal QGS crates. A narrowly scoped,
documented unsafe boundary exists in `qgs-vulkan` only for Vulkano external
memory import where no practical safe API is available. See `docs/safety.md`.

## External Synchronization Boundary

M1 Step 7 keeps shared resource identity and execution ordering separate:

```text
ResourceId
    represents
shared memory/resource identity

SyncId
    represents
producer-to-consumer GPU execution ordering
```

A synchronization primitive is not a buffer, and a buffer is not a semaphore.
Both are owned by the session that created them. Another session cannot export
or use them, and disconnecting the client drops any still-owned resources and
sync objects.

The Step 7 proof uses a Vulkan binary external semaphore exported as a Linux
sync FD. The producer submits GPU transfer work that fills the shared buffer and
signals the semaphore. The sync FD is transferred as a Linux transport
attachment with `SCM_RIGHTS`. The consumer imports the semaphore payload,
submits GPU work that waits on it, copies from the shared buffer into a
readback buffer, and then waits on a fence only to let the CPU inspect the
consumer result. `device.wait_idle()` and `queue.wait_idle()` are not used as
the producer-to-consumer dependency mechanism.

Step 7 proves cross-process GPU ordering for a minimal transfer operation. It
does not implement compute shaders, image/video resources, a video pipeline,
external semaphore reuse protocols, or concurrent producer/consumer scheduling.

## Compute Processing Proof

M1 Step 8 proves that an exported QGS buffer can be imported into a second
Vulkan context and used by a real Vulkan compute shader while preserving the
external-memory and external-synchronization architecture from Steps 6 and 7.
The proof uses a fixed QGS-owned shader inside `qgs-vulkan` that increments
`u32` values in a shared storage buffer.

This is not a public QGS compute API. The protocol does not expose shader
modules, SPIR-V blobs, descriptor binding, pipeline creation, dispatch commands,
or scheduler policy. QGS still reports and manages resources; it does not become
a replacement for Vulkan.

## Current Scope

The initial workspace contains an explicit v0.1 protocol wire encoding, minimal
session management, Linux Unix Domain Socket transport, and Vulkan-backed
device enumeration and static capability discovery through the `DeviceDiscovery`
abstraction. It reports compute queue limits, memory heap/type summaries, and
external-memory/synchronization mechanism availability. It can create and
destroy session-owned Vulkan-backed buffer resources and can export/import
external-memory FDs for explicitly exportable buffers on supported drivers. It
can also prove one-shot external GPU synchronization with Linux sync FDs on
supported drivers. It can run a private built-in compute proof against imported
shared buffers. It does not include an async runtime, daemonization, DRM/KMS,
image resources, video surfaces, video decode, video encode, a public compute
API, reusable semaphore workflows, workload scheduling, performance
benchmarking, telemetry, or free-memory reporting.

Capability discovery is static information reported by the backend. It is not a
measurement of current load, available/free VRAM, throughput, or scheduling
suitability. Interop mechanism availability means that the backend exposes an
API mechanism; it does not guarantee that every resource, image format, or usage
can be exported or imported with that mechanism.

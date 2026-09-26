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

## Current Scope

The initial workspace contains an explicit v0.1 protocol wire encoding, minimal
session management, Linux Unix Domain Socket transport, and Vulkan-backed
device enumeration and static capability discovery through the `DeviceDiscovery`
abstraction. It reports compute queue limits, memory heap/type summaries, and
external-memory/synchronization mechanism availability. It does not include an
async runtime, daemonization, DRM, DMA-BUF transfer/export, video decode, video
encode, GPU resource allocation, compute execution, workload scheduling,
performance benchmarking, telemetry, or free-memory reporting.

Capability discovery is static information reported by the backend. It is not a
measurement of current load, available/free VRAM, throughput, or scheduling
suitability. Interop mechanism availability means that the backend exposes an
API mechanism; it does not guarantee that every resource, image format, or usage
can be exported or imported with that mechanism.

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
device enumeration through the `DeviceDiscovery` abstraction. It does not
include an async runtime, daemonization, DRM, DMA-BUF, video decode, video
encode, GPU resource allocation, compute workloads, or workload scheduling.

# wwn-vms App Store / platform compliance

Honest, per-target posture for shipping a VM engine. These are constraints we
respect, not obstacles we route around.

| Target | VM support | Engine | Store posture |
| --- | --- | --- | --- |
| macOS (direct/notarized) | Yes | Virtualization.framework (microvm.nix + vfkit) + `wawona-vz` | Direct/notarized channel only. Needs `com.apple.security.virtualization`. |
| macOS (Mac App Store) | **Hidden** | - | MAS sandbox forbids spawning VMs; feature not exposed in MAS builds. |
| iOS | Yes | jitless QEMU-TCTI (UTM SE model) | Allowed: no JIT, no `Hypervisor.framework`, guest ships as bundled/ODR **data** (no downloaded executables). Precedent: UTM SE. |
| iPadOS | Yes | jitless QEMU-TCTI | Same as iOS. |
| visionOS | Yes | jitless QEMU-TCTI (shared iOS path) | Same as iOS. |
| tvOS | Limited | jitless QEMU-TCTI, minimal NixOS | Tight RAM ceiling; may degrade to OCI-management-only. |
| watchOS | **No** | - | Infeasible. wwn-vms exposes nothing here; container image management (if any) is `wwn-containers`. |
| Android | Yes | QEMU (TCG/JIT) + opportunistic KVM/AVF | JIT permitted on Android; Play-Store compliant. |

## Hard rules

- **NixOS-only guest.** The built-in VM boots prebuilt NixOS images only.
- **No JIT on Apple targets.** iOS/iPadOS/tvOS/visionOS use TCTI (translate-and-cache
  ahead of execution), never runtime JIT. This is the App Store ceiling and is
  not bypassed.
- **Guests are data.** Guest kernels/rootfs are bundled resources or On-Demand
  Resources - never downloaded executable code - on Apple targets.
- **MAS ships without VMs.** Anything requiring VM spawning is absent from Mac
  App Store builds.
- **GUI over waypipe + vsock**, not an emulated GPU/framebuffer, so the guest's
  Wayland session renders through Wawona.

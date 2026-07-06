# Vendored UTM engine sources (formerly the `wwn-utm` repo)

The QEMU-TCTI / Virtualization.framework engine pieces wwn-vms needs from UTM,
vendored **in-repo** so wwn-vms is self-contained. This replaces the separate
`github:Wawona/UTM` (`wwn-utm`) flake input, which pointed at a repo that was
never published — a phantom dependency that only evaluated from local caches.

## Provenance

- Upstream project: <https://github.com/utmapp/UTM>
- Vendored from the local Wawona fork at commit `51e1a7f44b63c6511c349aecb6619b6a7b6dbca1`
  (fork base: upstream `30c8202c` "CocoaSpice: re-introduce hack to reduce latency").
- To refresh: diff these files against upstream UTM and re-copy; paths below.

## Layout (mirrors UTM's own layout so scripts work unmodified)

| here | from UTM | why |
|---|---|---|
| `patches/` | `patches/` | dependency + **`qemu-10.0.2-utm.patch`** (the jitless TCTI/SE patch — the heart of the UTM SE model) + `sources` manifest |
| `scripts/build_dependencies.sh` | `scripts/` | builds QEMU + all deps per platform/arch (expects `../patches`, preserved) |
| `scripts/pack_dependencies.sh` | `scripts/` | packs built artifacts into frameworks |
| `sources/` | `Services/` | reference Swift/ObjC engine backends (QEMU process/system, VZ machine) |
| `xcschemes/iOS-SE.xcscheme` | `UTM.xcodeproj/.../xcschemes/` | the jitless (TCTI, no-JIT) build scheme — documents the SE build configuration |

## What Wawona does NOT take from UTM

UTM's app UI/document model. Wawona renders guests inside its own compositor
GUI (SwiftUI Machines shell + compositor host view); only the emulator engine
build machinery is reused. iOS/iPadOS/tvOS/visionOS use the TCTI (no-JIT,
App Store-safe) configuration; Android may enable TCG+JIT (permitted there).

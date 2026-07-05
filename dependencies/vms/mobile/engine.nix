# QEMU-TCTI mobile VM engine for iOS / iPadOS / visionOS / tvOS.
#
# On these targets there is no Hypervisor.framework, so the ceiling is the UTM
# SE model: jitless QEMU using TCTI (Tiny Code Threaded Interpreter) — an
# App-Store-approved precedent. The engine sources come from `wwn-utm` (the
# aligned UTM fork; see align-wwn-utm), which carries the TCTI patches and the
# iOS build machinery. We do NOT vendor a second QEMU here — wwn-utm is the one
# source of truth for the emulator.
#
# Build inputs (wired once wwn-utm is a flake input of wwn-vms):
#   * wwn-utm QEMU-TCTI xcframework (per Apple target/arch/simulator)
#   * a bundled minimal NixOS guest (./guest.nix) as ODR/bundled data
#   * wwn-waypipe (vsock Wayland transport) for the guest->Wawona GUI bridge
#   * wwn-toolchain cross toolchains (buildForIOS/…)
#
# Until wwn-utm is aligned + added as an input, this evaluates cleanly (so the
# registry merges and target enumeration works) but fails the build with a
# precise message rather than pretending to produce an emulator.
{
  pkgs,
  lib ? pkgs.lib,
  # The Apple platform this engine targets (informational; the real derivation
  # picks the matching wwn-utm xcframework slice).
  applePlatform ? "ios",
  # Set by wwn-vms' flake once wwn-utm is an input.
  wwn-utm ? null,
}:

if wwn-utm == null then
  throw ''
    wwn-vms mobile engine (QEMU-TCTI) for ${applePlatform} needs the aligned
    `wwn-utm` input (align-wwn-utm todo). Once wwn-utm exposes its jitless
    QEMU-TCTI xcframework, this derivation links it against the bundled NixOS
    guest (./guest.nix) + wwn-waypipe and produces the embeddable engine.
    No JIT, no Hypervisor.framework — TCTI is the honest ceiling (COMPLIANCE.md).
  ''
else
  # Placeholder for the real assembly once the input lands. Kept as a lambda so
  # the shape of the eventual derivation (guest + engine + transport) is visible.
  pkgs.runCommand "wwn-vms-mobile-engine-${applePlatform}" { } ''
    mkdir -p "$out"
    echo "assemble QEMU-TCTI (${applePlatform}) from wwn-utm + guest.nix here" > "$out/README"
  ''

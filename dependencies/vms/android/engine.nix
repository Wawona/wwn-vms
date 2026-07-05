# Android VM engine for wwn-vms.
#
# Android permits JIT, so unlike iOS this engine is NOT limited to TCTI:
#   * QEMU with TCG + JIT (fast software emulation), and
#   * opportunistic acceleration via the Android Virtualization Framework (AVF)
#     / KVM where the device+ROM expose it (Pixel 6+, GrapheneOS, etc.).
# Play-Store compliant (JIT is allowed on Android).
#
# The QEMU sources come from `wwn-utm` (same emulator as the Apple mobile path,
# built with JIT enabled for Android) cross-compiled through `wwn-toolchain`'s
# Android NDK toolchain. The guest is the same bundled minimal NixOS aarch64
# image (../mobile/guest.nix).
#
# Evaluates cleanly; throws with precise next-steps until `wwn-utm` is aligned
# and added as a wwn-vms input (align-wwn-utm).
{
  pkgs,
  lib ? pkgs.lib,
  # "qemu-jit" (portable, always available) or "avf" (needs device support).
  accel ? "qemu-jit",
  wwn-utm ? null,
}:

if wwn-utm == null then
  throw ''
    wwn-vms Android engine (${accel}) needs the aligned `wwn-utm` input
    (align-wwn-utm). It cross-compiles QEMU (TCG+JIT) through wwn-toolchain's
    Android NDK toolchain and boots ../mobile/guest.nix. AVF/KVM acceleration is
    used opportunistically where the device exposes it; QEMU-JIT is the portable
    fallback. JIT is permitted on Android (Play-Store compliant).
  ''
else
  pkgs.runCommand "wwn-vms-android-engine-${accel}" { } ''
    mkdir -p "$out"
    echo "assemble QEMU(${accel})/AVF from wwn-utm + mobile guest here" > "$out/README"
  ''

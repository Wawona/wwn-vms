# Per-target VM capability matrix + eval-time assertions ("capability lane").
# CI checks this with `nix eval .#lib.capabilities`; the asserts fail evaluation
# if the matrix ever drifts from the COMPLIANCE.md posture.
#
#   vm            can this target spawn a VM at all?
#   guestBundled  is the NixOS guest shipped as bundled/ODR data (Apple/Android)?
#   accel         "vz" | "tcti" | "qemu-jit" | null  (emulation lane)
let
  caps = {
    macos = { vm = true; guestBundled = false; accel = "vz"; };
    ios = { vm = true; guestBundled = true; accel = "tcti"; };
    ipados = { vm = true; guestBundled = true; accel = "tcti"; };
    tvos = { vm = true; guestBundled = true; accel = "tcti"; };
    visionos = { vm = true; guestBundled = true; accel = "tcti"; };
    watchos = { vm = false; guestBundled = false; accel = null; };
    android = { vm = true; guestBundled = true; accel = "qemu-jit"; };
  };
  targets = builtins.attrNames caps;
in
# watchOS never runs a VM.
assert caps.watchos.vm == false;
# macOS uses Virtualization.framework directly (native speed + Rosetta).
assert caps.macos.accel == "vz";
# Apple mobile has no Hypervisor.framework -> jitless TCTI ceiling (no JIT).
assert caps.ios.accel == "tcti" && caps.visionos.accel == "tcti";
# Android permits JIT, so QEMU can be faster than the iOS TCTI ceiling.
assert caps.android.accel == "qemu-jit";
# Any target with a VM either uses VZ (macOS) or bundles its guest as data.
assert builtins.all
  (t: !caps.${t}.vm || caps.${t}.accel == "vz" || caps.${t}.guestBundled)
  targets;
caps

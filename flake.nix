{
  description = "wwn-vms: Wawona's virtual-machine substrate. NixOS-only built-in guest, one engine per target: Virtualization.framework (microvm.nix + vfkit) on macOS; jitless QEMU-TCTI (UTM SE model) on iOS/iPadOS/tvOS/visionOS; QEMU/AVF on Android. SKELETON - real per-target engines are downstream (see README.md, COMPLIANCE.md).";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.url = "github:Wawona/wwn-toolchain";
    wwn-toolchain.inputs.nixpkgs.follows = "nixpkgs";
    wwn-toolchain.inputs.rust-overlay.follows = "rust-overlay";
    # The macOS built-in NixOS VM path (relocated from Wawona) uses microvm.nix
    # to build the guest + drive vfkit (Virtualization.framework).
    microvm.url = "github:microvm-nix/microvm.nix";
    microvm.inputs.nixpkgs.follows = "nixpkgs";
    # Mobile (iOS/iPadOS/tvOS/visionOS) + Android jitless QEMU-TCTI engine source:
    # the aligned UTM fork (wwn-utm). Local path input while pre-release; switched
    # to github:Wawona/wwn-utm once stable. Consumed by mobile/engine.nix +
    # android/engine.nix (their `wwn-utm` arg).
    wwn-utm.url = "path:/Users/8amps/Wawona/UTM";
    wwn-utm.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay, wwn-toolchain, microvm, wwn-utm, ... }:
    let
      darwinSystems = [ "x86_64-darwin" "aarch64-darwin" ];
      linuxSystems = [ "x86_64-linux" "aarch64-linux" ];
      allSystems = darwinSystems ++ linuxSystems;
      forAll = nixpkgs.lib.genAttrs allSystems;
      inherit (wwn-toolchain.lib) withPlatformVariants;

      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
        config = { allowUnfree = true; allowUnsupportedSystem = true; android_sdk.accept_license = true; };
      };

      dir = ./dependencies/vms;
    in
    {
      # Registry fragment merged into Wawona's client/machine registry. Entries
      # currently point at stubs that evaluate cleanly (so the registry merges and
      # CI can enumerate targets) but fail the build with a clear message until the
      # per-target engine lands.
      #
      #   nixos-vm     the NixOS-only built-in VM (the whole point of wwn-vms)
      #   vm-engine    the per-target hypervisor/emulator backend
      registryFragment = {
        nixos-vm = withPlatformVariants {
          macos = dir + "/stub.nix";
          ios = dir + "/stub.nix";
          ipados = dir + "/stub.nix";
          tvos = dir + "/stub.nix";
          visionos = dir + "/stub.nix";
          watchos = dir + "/stub.nix";
          android = dir + "/stub.nix";
          wearos = dir + "/stub.nix";
        };
        vm-engine = withPlatformVariants {
          macos = dir + "/stub.nix";
          ios = dir + "/stub.nix";
          ipados = dir + "/stub.nix";
          tvos = dir + "/stub.nix";
          visionos = dir + "/stub.nix";
          watchos = dir + "/stub.nix";
          android = dir + "/stub.nix";
          wearos = dir + "/stub.nix";
        };
      };

      # The aligned UTM fork, surfaced for downstream engine assembly + so the
      # input is a concrete dependency (mobile/engine.nix + android/engine.nix
      # take `wwn-utm` and use wwn-utm.lib for the QEMU-TCTI sources/patches).
      lib.wwn-utm = wwn-utm;

      # Bundled minimal NixOS guest for the mobile QEMU-TCTI engine
      # (iOS/iPadOS/visionOS/tvOS). Evaluable everywhere; the kernel/rootfs
      # artifacts build on the aarch64-linux builder and ship as ODR/bundled data
      # (COMPLIANCE.md). The engine that boots it is dependencies/vms/mobile/engine.nix
      # (sourced from wwn-utm; see align-wwn-utm).
      nixosConfigurations.wawona-mobile-guest =
        import ./dependencies/vms/mobile/guest.nix { inherit nixpkgs; };

      formatter = forAll (system: (pkgsFor system).nixfmt-rfc-style);
    };
}

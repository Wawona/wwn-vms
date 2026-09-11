# Packages the verified iOS-TCI (jitless QEMU-TCTI) engine sysroot.
#
# The cross-build is Xcode-driven and impure (see flake devShell `utm-engine`).
# This derivation either:
#   * copies a prebuilt sysroot when WAWONA_UTM_SYSROOT points at one, or
#   * runs the build in-place via `nix develop …#utm-engine` on Darwin hosts.
{
  pkgs,
  lib ? pkgs.lib,
  utm,
  self,
  system ? pkgs.stdenv.hostPlatform.system,
  platform ? "ios-tci",
  arch ? "arm64",
  qemuTargetList ? null,
}:
let
  sysrootEnv = builtins.getEnv "WAWONA_UTM_SYSROOT";
  sysrootCandidate =
    if sysrootEnv != "" && builtins.pathExists sysrootEnv then
      builtins.path {
        path = sysrootEnv;
        name = "wawona-utm-sysroot-${platform}-${arch}";
      }
    else
      null;
  scheme =
    if platform == "ios-tci" then "iOS-TCI"
    else if platform == "ios" then "iOS"
    else lib.toUpper (lib.head (lib.splitString "-" platform));
  expectedName = "sysroot-${scheme}-${arch}";
  # Same set as flake `utm-engine`. Must be a derivation input so
  # `nix develop --ignore-env` cannot drop the interpreter mid-build
  # (glib meson `python.find_installation` then posix_spawns a GC'd path).
  pythonEnv = pkgs.python3.withPackages (ps: with ps; [
    six
    pyparsing
    tomli
    setuptools
    pyyaml
    distlib
    mako
    packaging
    sphinx
    sphinx-rtd-theme
  ]);
  # Same as flake `utm-engine`. `--ignore-env` drops mkShell exports, so
  # mesa host meson cannot find xrandr unless these are kept from here.
  llvmHost = pkgs.symlinkJoin {
    name = "llvm-host";
    paths = with pkgs.llvmPackages; [
      llvm
      llvm.dev
      llvm.lib
      clang-unwrapped
      clang-unwrapped.dev
      clang-unwrapped.lib
    ];
    postBuild = ''
      rm -f $out/bin/llvm-config
      cat > $out/bin/llvm-config <<EOF
      #!/bin/sh
      exec_prefix_fixup() { ${pkgs.gnused}/bin/sed -e "s|${pkgs.llvmPackages.llvm.lib}|$out|g" -e "s|${pkgs.llvmPackages.llvm.dev}|$out|g"; }
      ${pkgs.llvmPackages.llvm.dev}/bin/llvm-config "\$@" | exec_prefix_fixup
      EOF
      ${pkgs.gnused}/bin/sed -i 's/^      //' $out/bin/llvm-config
      chmod +x $out/bin/llvm-config
    '';
  };
  mesaHostPkgConfigPath = lib.concatStringsSep ":" (
    lib.concatMap (p: [ "${p}/lib/pkgconfig" "${p}/share/pkgconfig" ]) [
      pkgs.libclc.dev
      pkgs.libclc
      pkgs.spirv-tools.dev
      pkgs.spirv-tools
      pkgs.libxcb.dev
      pkgs.libx11.dev
      pkgs.libxrandr.dev
      pkgs.libxrender.dev
      pkgs.libxext.dev
      pkgs.libxfixes.dev
      pkgs.libxau.dev
      pkgs.libxdmcp.dev
      pkgs.libxshmfence.dev
      pkgs.xorgproto
    ]
  );
in
if sysrootCandidate != null then
  pkgs.runCommand "wwn-vms-mobile-engine-${platform}-${arch}" { } ''
    cp -a ${sysrootCandidate} $out
    test -d "$out/Frameworks" || { echo "missing Frameworks/ in sysroot" >&2; exit 1; }
  ''
else if !lib.hasPrefix "aarch64-darwin" system
  && !lib.hasPrefix "x86_64-darwin" system then
  throw ''
    wwn-vms mobile engine pack (${platform}/${arch}) must be built on Darwin
    (needs Xcode + Metal toolchain). On macOS:
      nix develop ${self}#utm-engine -c /bin/sh ${utm.dir}/scripts/build_dependencies.sh -p ${platform} -a ${arch}
    then:
      WAWONA_UTM_SYSROOT=$PWD/${expectedName} nix build .#packages.$(nix config show --json | jq -r .'"system"').wwn-vms-mobile-engine-${platform}
  ''
else
  pkgs.runCommand "wwn-vms-mobile-engine-${platform}-${arch}" {
    nativeBuildInputs = [
      pkgs.nix
      pkgs.coreutils
      pkgs.cacert
      pkgs.meson
      pkgs.ninja
      pkgs.cmake
      pkgs.bison
      pkgs.pkg-config
      pkgs.gettext
      pkgs.nasm
      pkgs.curl
      pkgs.git
      pkgs.glslang
      pkgs.spirv-tools
      pythonEnv
    ];
    __noChroot = true;
  } ''
    export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
    work=$(mktemp -d)
    export HOME="$work/home"
    export XDG_CACHE_HOME="$work/cache"
    export TMPDIR="$work/tmp"
    export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
    export PYTHON3="${pythonEnv}/bin/python3"
    export WWN_LLVM_HOST_PREFIX="${llvmHost}"
    export MESA_HOST_PKG_CONFIG_PATH="${mesaHostPkgConfigPath}"
    mkdir -p "$HOME" "$XDG_CACHE_HOME" "$TMPDIR"
    cd "$work"
    ${lib.optionalString (qemuTargetList != null) ''
      export WWN_QEMU_TARGET_LIST=${lib.escapeShellArg qemuTargetList}
    ''}
    ${pkgs.nix}/bin/nix develop \
      --ignore-env \
      --keep-env-var HOME \
      --keep-env-var XDG_CACHE_HOME \
      --keep-env-var TMPDIR \
      --keep-env-var PATH \
      --keep-env-var PYTHON3 \
      --keep-env-var WWN_LLVM_HOST_PREFIX \
      --keep-env-var MESA_HOST_PKG_CONFIG_PATH \
      ${lib.optionalString (qemuTargetList != null) "--keep-env-var WWN_QEMU_TARGET_LIST"} \
      ${self}#utm-engine \
      -c /bin/sh ${utm.dir}/scripts/build_dependencies.sh -p ${platform} -a ${arch}
    test -d ${expectedName} || { echo "expected ${expectedName} after build" >&2; exit 1; }
    cp -a ${expectedName} $out
  ''

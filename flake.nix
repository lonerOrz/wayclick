{
  description = "WayClick – low-latency input sound engine (Rust)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          # Rust toolchain that also knows the Windows GNU target, for
          # cross-building an .exe from Linux.
          rustWin =
            pkgs.rust-bin.stable.latest.default.override
              {
                targets = [ "x86_64-pc-windows-gnu" ];
              };

          # mingw-w64 toolchain provides the linker + Windows import libs.
          mingwPkgs = pkgs.pkgsCross.mingwW64;

          nativeLibs = [
            pkgs.pkg-config
            pkgs.alsa-lib
            pkgs.udev
            pkgs.libevdev
          ];

          # macOS needs the CoreGraphics/CoreFoundation frameworks for the objc2
          # CGEventTap backend (sandboxed Nix builds link against these).
          darwinFrameworks = pkgs.lib.optionals pkgs.stdenv.isDarwin (
            with pkgs.darwin.apple_sdk.frameworks;
            [
              CoreGraphics
              CoreFoundation
            ]
          );

          darwinInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin darwinFrameworks;

          wayclick = pkgs.rustPlatform.buildRustPackage {
            pname = "wayclick";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs =
              pkgs.lib.optionals pkgs.stdenv.isLinux nativeLibs
              ++ darwinInputs;
            buildInputs =
              pkgs.lib.optionals pkgs.stdenv.isLinux nativeLibs
              ++ darwinInputs;

            postInstall = ''
              mkdir -p $out/share/wayclick
              cp -r ${./assets/default} $out/share/wayclick/config
            '';

            meta = {
              description = "Low-latency input sound engine";
              mainProgram = "wayclick";
            };
          };

          # Cross-built Windows .exe (runs from Linux via `nix build .#winExe`).
          winExe =
            pkgs.lib.optionalAttrs pkgs.stdenv.isLinux
              (mingwPkgs.rustPlatform.buildRustPackage {
            pname = "wayclick-win";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;

            cargoLock.lockFile = ./Cargo.lock;

            # Use the toolchain that bundles the windows-gnu target.
            cargo = rustWin;
            rustc = rustWin;

            CARGO_BUILD_TARGET = "x86_64-pc-windows-gnu";

            # mingw linker + Windows import libs for the cpal/windows-sys deps.
            nativeBuildInputs = [ mingwPkgs.buildPackages.gcc ];
            buildInputs = [ ];

            # bundle the default config so the .exe finds assets/default next to it.
            postInstall = ''
              mkdir -p $out/assets/default
              cp -r ${./assets/default}/* $out/assets/default/
            '';

            meta = {
              description = "Low-latency input sound engine (Windows .exe)";
              mainProgram = "wayclick.exe";
            };
          });
        in
        {
          packages.default = wayclick;
          packages.winExe = winExe;
          checks.unit-tests = pkgs.rustPlatform.buildRustPackage {
            pname = "wayclick-tests";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs =
              pkgs.lib.optionals pkgs.stdenv.isLinux nativeLibs
              ++ darwinInputs;
            buildInputs =
              pkgs.lib.optionals pkgs.stdenv.isLinux nativeLibs
              ++ darwinInputs;
            # Skip the install phase; we only want `cargo test` from checkPhase.
            doInstallCargoBinaries = false;
            installPhase = "touch $out";
          };

          devShells.default = pkgs.mkShell {
            buildInputs = [
              pkgs.cargo
              pkgs.rustc
              pkgs.clippy
              pkgs.rustfmt
            ]
            ++ nativeLibs;
          };

          apps.default = {
            type = "app";
            program = "${wayclick}/bin/wayclick";
          };

          formatter = pkgs.writeShellScriptBin "wayclick-fmt" ''
            set -euo pipefail
            cargo fmt --all
            ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt flake.nix
            ${pkgs.prettier}/bin/prettier --write . --ignore-path .gitignore
          '';
        };
    in
    let
      lib = nixpkgs.lib;
      mk = attr: lib.genAttrs systems (s: (perSystem s).${attr});
    in
    {
      packages = mk "packages";
      checks = mk "checks";
      devShells = mk "devShells";
      apps = mk "apps";
      formatter = mk "formatter";
    };
}

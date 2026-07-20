{
  description = "WayClick – low-latency input sound engine (Rust)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
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
          pkgs = import nixpkgs { inherit system; };

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
        in
        {
          packages.default = wayclick;
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
              ${pkgs.prettier}/bin/prettier --write \
                "assets/**/*.json" ".github/**/*.yml"
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

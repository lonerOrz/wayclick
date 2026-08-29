{
  description = "WayClick – low-latency input sound engine";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      perSystem =
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };

          rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
              "clippy"
              "rustfmt"
            ];
            targets = [ "x86_64-pc-windows-gnu" ];
          };

          rustPlatform = pkgs.makeRustPlatform {
            rustc = rustToolchain;
            cargo = rustToolchain;
          };

          linuxNativeLibs = with pkgs; [
            pkg-config
            alsa-lib
            udev
            libevdev
          ];

          darwinFrameworks = with pkgs.darwin.apple_sdk.frameworks; [
            CoreGraphics
            CoreFoundation
          ];

          commonNativeBuildInputs =
            with pkgs;
            [ pkg-config ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux linuxNativeLibs
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin darwinFrameworks;

          commonBuildInputs =
            pkgs.lib.optionals pkgs.stdenv.isLinux linuxNativeLibs
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin darwinFrameworks;

          buildWayclick =
            {
              pname,
              doCheck ? true,
              ...
            }@args:
            rustPlatform.buildRustPackage (
              {
                inherit pname;
                version = "0.1.0";
                src = pkgs.lib.cleanSource ./.;
                cargoLock.lockFile = ./Cargo.lock;
                inherit commonNativeBuildInputs commonBuildInputs doCheck;
              }
              // args
            );

          wayclick = buildWayclick {
            pname = "wayclick";
            postInstall = ''
              mkdir -p $out/share/wayclick
              cp -r ${./assets/default} $out/share/wayclick/config
            '';
            meta = {
              description = "Low-latency input sound engine";
              mainProgram = "wayclick";
            };
          };

          fmtScript = pkgs.writeShellScriptBin "formatter" ''
            set -e
            for arg in "$@"; do
              if [ -d "$arg" ]; then
                ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt "$arg"
                ${pkgs.prettier}/bin/prettier --write "$arg"
                find "$arg" -name "*.rs" -type f -exec ${rustToolchain}/bin/rustfmt {} + || true
              else
                case "$arg" in
                  *.nix) ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt "$arg" ;;
                  *.rs) ${rustToolchain}/bin/rustfmt "$arg" ;;
                  *.md|*.yaml|*.yml) ${pkgs.prettier}/bin/prettier --write "$arg" ;;
                esac
              fi
            done
          '';

        in
        {
          packages.default = wayclick;

          devShells.default = pkgs.mkShell {
            inputsFrom = [ wayclick ];

            nativeBuildInputs = with pkgs; [
              rustToolchain
              zig
              cargo-zigbuild
            ];

            buildInputs = [
              pkgs.pkgsCross.mingwW64.windows.pthreads
            ];

            env = {
              CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${pkgs.pkgsCross.mingwW64.windows.pthreads}/lib";
            };
          };

          apps.default = {
            type = "app";
            program = "${wayclick}/bin/wayclick";
          };

          formatter = fmtScript;
        };
    in
    {
      packages = nixpkgs.lib.genAttrs systems (s: (perSystem s).packages);
      devShells = nixpkgs.lib.genAttrs systems (s: (perSystem s).devShells);
      apps = nixpkgs.lib.genAttrs systems (s: (perSystem s).apps);
      formatter = nixpkgs.lib.genAttrs systems (s: (perSystem s).formatter);
    };
}

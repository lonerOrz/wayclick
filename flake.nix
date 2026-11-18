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
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      forAllSystems = lib.genAttrs systems;

      mkPkgs =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

      mkRust =
        pkgs:
        pkgs.rust-bin.nightly.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
            "rustfmt"
          ];
          targets = [ "x86_64-pc-windows-gnu" ];
        };

      # Platform-specific native dependencies
      mkDeps =
        pkgs:
        let
          linuxDeps = with pkgs; [
            alsa-lib
            udev
            libevdev
          ];

          darwinDeps = with pkgs.darwin.apple_sdk.frameworks; [
            CoreGraphics
            CoreFoundation
          ];
        in
        {
          nativeBuildInputs = [
            pkgs.pkg-config
          ]
          ++ lib.optionals pkgs.stdenv.isLinux linuxDeps
          ++ lib.optionals pkgs.stdenv.isDarwin darwinDeps;

          buildInputs =
            lib.optionals pkgs.stdenv.isLinux linuxDeps ++ lib.optionals pkgs.stdenv.isDarwin darwinDeps;
        };

      mkWayclick =
        {
          pkgs,
          rustPlatform,
        }:
        rustPlatform.buildRustPackage {
          pname = "wayclick";
          version = builtins.head (
            builtins.match ".*version = \"([0-9]+\\.[0-9]+\\.[0-9]+)\".*" (builtins.readFile ./Cargo.toml)
          );

          src = lib.cleanSource ./.;

          cargoLock.lockFile = ./Cargo.lock;

          inherit (mkDeps pkgs) nativeBuildInputs buildInputs;

          doCheck = true;

          postInstall = ''
            mkdir -p $out/share/wayclick
            cp -r ${./assets/default} $out/share/wayclick/config
          '';

          meta = {
            description = "Low-latency input sound engine";
            mainProgram = "wayclick";
          };
        };

      mkFormatter =
        {
          pkgs,
          rust,
        }:
        pkgs.writeShellScriptBin "formatter" ''
          set -euo pipefail

          for arg in "$@"; do
            if [ -d "$arg" ]; then
              ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt "$arg"
              ${pkgs.prettier}/bin/prettier --write "$arg"
              find "$arg" -name "*.rs" -type f -exec ${rust}/bin/rustfmt {} + || true
            else
              case "$arg" in
                *.nix)
                  ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt "$arg"
                  ;;
                *.rs)
                  ${rust}/bin/rustfmt "$arg"
                  ;;
                *.md|*.yaml|*.yml)
                  ${pkgs.prettier}/bin/prettier --write "$arg"
                  ;;
              esac
            fi
          done
        '';

      perSystem =
        system:
        let
          pkgs = mkPkgs system;
          rust = mkRust pkgs;

          rustPlatform = pkgs.makeRustPlatform {
            cargo = rust;
            rustc = rust;
          };

          wayclick = mkWayclick {
            inherit pkgs rustPlatform;
          };

          formatter = mkFormatter {
            inherit pkgs rust;
          };

          deps = mkDeps pkgs;
        in
        {
          packages.default = wayclick;

          devShells.default = pkgs.mkShell {
            inputsFrom = [ wayclick ];

            nativeBuildInputs = [
              rust
              pkgs.zig
              pkgs.cargo-zigbuild
            ]
            ++ deps.nativeBuildInputs;

            buildInputs = [
              pkgs.pkgsCross.mingwW64.windows.pthreads
            ]
            ++ deps.buildInputs;

            env = {
              CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${pkgs.pkgsCross.mingwW64.windows.pthreads}/lib";
            };
          };

          apps.default = {
            type = "app";
            program = "${wayclick}/bin/wayclick";
          };

          inherit formatter;
        };

      systemOutputs = forAllSystems perSystem;

      mapSystemOutput = name: forAllSystems (system: systemOutputs.${system}.${name});
    in
    {
      packages = mapSystemOutput "packages";
      devShells = mapSystemOutput "devShells";
      apps = mapSystemOutput "apps";
      formatter = mapSystemOutput "formatter";
    };
}

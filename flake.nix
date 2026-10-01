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

      eachSystem =
        nixpkgs.lib.genAttrs systems;


      mkPkgs =
        system:
        import nixpkgs {
          inherit system;
          overlays = [
            rust-overlay.overlays.default
          ];
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

          targets = [
            "x86_64-pc-windows-gnu"
          ];
        };


      mkDeps =
        pkgs:

        let
          lib = pkgs.lib;

          linuxDeps = with pkgs; [
            pkg-config
            alsa-lib
            udev
            libevdev
          ];

          darwinDeps =
            with pkgs.darwin.apple_sdk.frameworks; [
              CoreGraphics
              CoreFoundation
            ];

        in
        {
          nativeBuildInputs =
            [
              pkgs.pkg-config
            ]
            ++ lib.optionals pkgs.stdenv.isLinux linuxDeps
            ++ lib.optionals pkgs.stdenv.isDarwin darwinDeps;


          buildInputs =
            lib.optionals pkgs.stdenv.isLinux linuxDeps
            ++ lib.optionals pkgs.stdenv.isDarwin darwinDeps;
        };


      mkWayclick =
        {
          pkgs,
          rustPlatform,
        }:

        rustPlatform.buildRustPackage {
          pname = "wayclick";
          version = "0.1.0";

          src = nixpkgs.lib.cleanSource ./.;

          cargoLock.lockFile = ./Cargo.lock;

          inherit
            (mkDeps pkgs)
            nativeBuildInputs
            buildInputs;

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
          set -e

          for arg in "$@"; do

            if [ -d "$arg" ]; then

              ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt "$arg"

              ${pkgs.prettier}/bin/prettier \
                --write "$arg"

              find "$arg" \
                -name "*.rs" \
                -type f \
                -exec ${rust}/bin/rustfmt {} + || true

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

          rustPlatform =
            pkgs.makeRustPlatform {
              cargo = rust;
              rustc = rust;
            };


          wayclick =
            mkWayclick {
              inherit pkgs rustPlatform;
            };


          formatter =
            mkFormatter {
              inherit pkgs rust;
            };

        in
        {
          packages.default = wayclick;


          devShells.default =
            pkgs.mkShell {

              inputsFrom = [
                wayclick
              ];


              nativeBuildInputs =
                [
                  rust
                  pkgs.zig
                  pkgs.cargo-zigbuild
                ]
                ++ (mkDeps pkgs).nativeBuildInputs;


              buildInputs =
                [
                  pkgs.pkgsCross.mingwW64.windows.pthreads
                ]
                ++ (mkDeps pkgs).buildInputs;


              env = {
                CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS =
                  "-L native=${pkgs.pkgsCross.mingwW64.windows.pthreads}/lib";
              };
            };


          apps.default = {
            type = "app";
            program = "${wayclick}/bin/wayclick";
          };


          inherit formatter;
        };


      systemOutputs =
        eachSystem perSystem;


      mapSystemOutput =
        name:

        eachSystem (
          system:
            systemOutputs.${system}.${name}
        );


    in
    {
      packages =
        mapSystemOutput "packages";

      devShells =
        mapSystemOutput "devShells";

      apps =
        mapSystemOutput "apps";

      formatter =
        mapSystemOutput "formatter";
    };
}

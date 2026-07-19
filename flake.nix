{
  description = "WayClick Elite – low-latency input sound engine";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      forEachSystem = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
    in
    {
      # `nix fmt` — one command formats the whole project.
      formatter = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        pkgs.writeShellScriptBin "fmt" ''
          set -euo pipefail
          ${pkgs.black}/bin/black src tests template .github
          ${pkgs.prettier}/bin/prettier --write "**/*.{json,yaml,yml,md}"
          ${pkgs.nixpkgs-fmt}/bin/nixpkgs-fmt flake.nix
        ''
      );

      devShells = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs =
              with pkgs;
              [
                python311
                python311Packages.pygame-ce
                python311Packages.pyinstaller
                prettier
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
                python311Packages.evdev
              ];

            shellHook = ''
              echo "WayClick development environment ready!"
              echo "Format:  nix fmt   (black src tests template .github && prettier '**/*.{json,yaml,yml,md}')"
            '';
          };
        }
      );

      # `nix build` / `nix run .#wayclick` — packages the engine from src.
      packages = forEachSystem (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          python = pkgs.python311;
        in
        {
          default = pkgs.stdenv.mkDerivation {
            pname = "wayclick";
            version = "0.1";

            src = pkgs.lib.cleanSource ./.;

            nativeBuildInputs = [ pkgs.makeWrapper ];
            propagatedBuildInputs =
              [ pkgs.python311Packages.pygame-ce ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.python311Packages.evdev ];

            buildPhase = ''
              mkdir -p $out/lib/wayclick
              cp -r src/* $out/lib/wayclick/
              # Skip __pycache__ copied from the working tree.
              rm -rf $out/lib/wayclick/__pycache__
              cp -r template $out/lib/wayclick/template
            '';

            installPhase = ''
              mkdir -p $out/bin
              cat > $out/bin/wayclick <<EOF
              #!/bin/sh
              export PYTHONPATH=$out/lib/wayclick:\''\${PYTHONPATH:+:\$PYTHONPATH}
              exec ${python}/bin/python3 -m runner_cross_platform "\$@"
              EOF
              chmod +x $out/bin/wayclick
            '';

            meta = {
              description = "Low-latency input sound engine";
              mainProgram = "wayclick";
            };
          };
        }
      );
    };
}

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
                python310
                python310Packages.pygame-ce
                python310Packages.pyinstaller
                prettier
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
                python310Packages.evdev
              ];

            shellHook = ''
              echo "WayClick development environment ready!"
              echo "Format:  nix fmt   (black src tests template .github && prettier '**/*.{json,yaml,yml,md}')"
            '';
          };
        }
      );
    };
}

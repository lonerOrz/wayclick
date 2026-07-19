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
          ${pkgs.shfmt}/bin/shfmt -l -w src/wayclick.sh
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
                shfmt
                prettier
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
                python310Packages.evdev
              ];

            shellHook = ''
              echo "WayClick development environment ready!"
              echo "Format:  black src tests template .github && shfmt -l -w src/wayclick.sh && prettier --write '**/*.{json,yaml,yml,md}'"
            '';
          };
        }
      );
    };
}

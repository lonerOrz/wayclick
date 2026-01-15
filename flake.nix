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
                python313
                python313Packages.pygame-ce
                python313Packages.pyinstaller
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
                python313Packages.evdev
              ];

            shellHook = ''
              echo "WayClick development environment ready!"
            '';
          };
        }
      );
    };
}

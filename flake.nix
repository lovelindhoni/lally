{
  description = "lally";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";
    flake-root.url = "github:srid/flake-root";
    git-hooks.url = "github:cachix/git-hooks.nix";
    git-hooks.flake = false;
    systems.url = "github:nix-systems/default";
  };
  outputs =
    inputs@{
      self,
      nixpkgs,
      flake-parts,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        inputs.flake-root.flakeModule
      ]
      ++ (
        with builtins;
        map (fn: ./nix/flake/${fn}) (
          filter (f: nixpkgs.lib.hasSuffix ".nix" f) (attrNames (readDir ./nix/flake))
        )
      );
      perSystem =
        {
          config,
          self',
          inputs',
          pkgs,
          system,
          lib,
          ...
        }:
        {
          formatter = pkgs.nixfmt;
          flake-root.projectRootFile = "flake.nix";
          checks = {
            lally-build = self'.packages.lally;
          };
        };
      flake = { };
    };
}

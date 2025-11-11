{ inputs, ... }:
{
  imports = [
    (inputs.git-hooks + /flake-module.nix)
  ];
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
      pre-commit = {
        check.enable = true;
        settings = {
          src = inputs.self;
          hooks = {
            nixfmt.enable = true;
            rustfmt.enable = true;
            trailing-whitespace = {
              enable = true;
              name = "trim trailing whitespace";
              entry = "${pkgs.python3.pkgs.pre-commit-hooks}/bin/trailing-whitespace-fixer";
              types = [ "text" ];
            };
            end-of-file-fixer = {
              enable = true;
              name = "fix end of files";
              entry = "${pkgs.python3.pkgs.pre-commit-hooks}/bin/end-of-file-fixer";
              types = [ "text" ];
            };
          };
        };
      };
    };
}

{ inputs, ... }:
{
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
      devShells.default = pkgs.mkShell {
        name = "lally-devshell";
        meta.description = "Lally Development Environment";
        inputsFrom = [
          config.flake-root.devShell
          config.pre-commit.devShell
        ];
        packages = with pkgs; [
          nixd
          nixfmt
          rustc
          cargo
          rustfmt
          clippy
          rust-analyzer
          pkg-config
          protobuf
          openssl
          just
          gdb
          lldb
          gcc
          stdenv.cc
        ];

        shellHook = ''
          echo "rustc version: `rustc --version`"
          echo "cargo version: `cargo --version`"
          export RUST_BACKTRACE=1
          export PROTOC="${pkgs.protobuf}/bin/protoc"
          export PROTOC_INCLUDE="${pkgs.protobuf}/include"
        '';
        RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
      };
    };
}

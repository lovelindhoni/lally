{ inputs, ... }:
{
  perSystem =
    {
      config,
      self',
      inputs',
      pkgs,
      system,
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
        packages =
          with pkgs;
          [
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
          ]
          ++ lib.optionals stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
            darwin.apple_sdk.frameworks.SystemConfiguration
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

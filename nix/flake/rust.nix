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
      packages = {
        default = self'.packages.lally;
        lally = lib.mkDefault (
          pkgs.rustPlatform.buildRustPackage rec {
            pname = "lally";
            version = "0.2.0";
            src = lib.cleanSourceWith {
              src = inputs.self;
              filter =
                path: type:
                let
                  basePath = baseNameOf path;
                  parentPath = baseNameOf (dirOf path);
                in
                (lib.hasSuffix ".rs" path)
                || (lib.hasSuffix ".toml" path && basePath == "Cargo.toml")
                || (lib.hasSuffix ".lock" path && basePath == "Cargo.lock")
                || (basePath == "build.rs")
                || (lib.hasSuffix ".proto" path)
                || (type == "directory" && (basePath == "src" || basePath == "proto" || parentPath == "src"));
            };
            cargoLock = {
              lockFile = "${inputs.self}/Cargo.lock";
            };
            nativeBuildInputs = with pkgs; [
              pkg-config
              protobuf
            ];
            buildInputs = with pkgs; [
              openssl
            ];
            PROTOC = "${pkgs.protobuf}/bin/protoc";
            PROTOC_INCLUDE = "${pkgs.protobuf}/include";
            meta = with lib; {
              description = "A distributed in-memory key-value database.";
              homepage = "https://github.com/lovelindhoni/lally";
              license = licenses.mit;
            };
          }
        );
      };
    };
}

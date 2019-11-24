{ pkgs ? import <nixpkgs> {} }:
with builtins;
with pkgs;

let
  rust = nur.repos.mozilla.rustChannels.beta.rust;

  buildRustPackage' = rustPlatform.buildRustPackage.override {
    rustc = rust;
    cargo = rust;
  };
in buildRustPackage' rec {
  pname = "nix-weather";
  version = "0.1.0";

  ignore = map toString [
    ./target
  ];
  src = filterSource (path: type: !elem path ignore) ./.;

  nativeBuildInputs = [ pkgconfig ];
  buildInputs = [ openssl ];

  RUST_BACKTRACE = "1";
  cargoSha256 = "05kj65s1a2kygrw4jz2cg1jw00yxhgj8mdrki9x2i331v8h4kxg8";
}

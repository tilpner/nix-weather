{ pkgs ? import <nixpkgs> {} }:
with builtins;
with pkgs;

let
  nur = import (builtins.fetchTarball "https://github.com/nix-community/NUR/archive/master.tar.gz") {
    inherit pkgs;
  };

  rust = nur.repos.mozilla.rustChannelOf {
    date = "2019-11-07"; channel = "stable";
  };

  buildRustPackage' = rustPlatform.buildRustPackage.override {
    rustc = rust.rust;
    cargo = rust.rust;
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

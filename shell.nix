with import <nixpkgs> {};

let
  rust = nur.repos.mozilla.rustChannels.beta.rust;
in mkShell {
  buildInputs = [ rust pkgconfig openssl ];
  # inherit (import ./default.nix {}) buildInputs nativeBuildInputs;
}

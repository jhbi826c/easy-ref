{
  description = "easy-ref: reference manager (Rust + Leptos, static web app)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          targets = [ "wasm32-unknown-unknown" ];
        };

        # Must match the wasm-bindgen version pinned in Cargo.toml.
        wasm-bindgen-cli = pkgs.wasm-bindgen-cli;

        webTools = [
          rustToolchain
          pkgs.trunk
          wasm-bindgen-cli
          pkgs.binaryen
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = webTools ++ [
            (pkgs.texliveSmall.withPackages (ps: [ ps.ieeetran ps.acmart ]))
          ];
        };

        devShells.ci = pkgs.mkShell {
          packages = webTools;
        };
      });
}

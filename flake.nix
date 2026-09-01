{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";

    kani-flake.url = "github:trantorian1/kani-flake";
    kani-flake.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    nixpkgs,
    kani-flake,
    ...
  }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs {
      inherit system;
      overlays = [
        kani-flake.overlays.default
      ];
    };
  in {
    devShells.${system}.default = pkgs.mkShell {
      buildInputs = with pkgs; [
        kani
        cargo-bolero

        (rust-bin.override {
          extensions = [
            "rust-analyzer"
            "rust-src"
          ];
        })
      ];
    };
  };
}

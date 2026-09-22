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
    pkgs = nixpkgs.legacyPackages.${system};
    kaniPackages = kani-flake.packages.${system};
  in {
    inherit pkgs;

    devShells.${system}.default = pkgs.mkShell {
      buildInputs = with pkgs; [
        cargo-bolero

        kaniPackages.kani
        (kaniPackages.rust-bin.override {
          extensions = [
            "rust-analyzer"
            "rust-src"
          ];
        })
      ];
    };
  };
}

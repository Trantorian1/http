{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";

    kani-flake.url = "github:trantorian1/kani-flake";
    kani-flake.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    kani-flake,
    ...
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    kaniPackages = kani-flake.packages.${system};
  in {
    inherit pkgs;

    packages.${system}.default = pkgs.buildEnv {
      name = "opencode-sandbox";
      paths = with pkgs; [
        opencode
        ripgrep
        python3

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

    devShells.${system}.default = pkgs.mkShell {
      buildInputs = [self.packages.${system}.default];
    };
  };
}

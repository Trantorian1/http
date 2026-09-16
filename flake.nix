{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";

    kani-flake.url = "github:trantorian1/kani-flake";
    kani-flake.inputs.nixpkgs.follows = "nixpkgs";

    opencode-sandbox.url = "github:OpencodeSandbox/opencode-sandbox";
  };

  outputs = {
    self,
    nixpkgs,
    kani-flake,
    opencode-sandbox,
    ...
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    kaniPackages = kani-flake.packages.${system};
  in {
    packages.${system} = rec {
      sandbox = opencode-sandbox.packages.${system}.sandbox.override {
        opencode-sandbox = {
          git.remote.url = "https://github.com/Trantorian1/http.git";
          git.shutdown.pushOnExit = false;
          git.withLocalChanges = true;

          limits.mem = 32768;
          limits.vcpu = 8;

          forwardPorts = [8888];
          env.extend = [devenv];
        };
      };

      devenv = pkgs.buildEnv {
        name = "devenv";
        paths = with pkgs; [
          cargo-bolero

          kaniPackages.kani
          (kaniPackages.rust-bin.override {
            extensions = [
              "rust-analyzer"
              "rust-src"
              "rustfmt"
            ];
          })
        ];
      };
    };

    devShells.${system}.default = pkgs.mkShell {
      buildInputs = [self.packages.${system}.devenv];
    };
  };
}

{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    nixlib.url = "github:nix-util/nixlib";

    kani-flake.url = "github:trantorian1/kani-flake";
    kani-flake.inputs.nixpkgs.follows = "nixpkgs";

    opencode-sandbox.url = "github:OpencodeSandbox/opencode-sandbox?ref=feat/opencodev2";
    opencode-sandbox.inputs.nixpkgs.follows = "nixpkgs";
    opencode-sandbox.inputs.nixlib.follows = "nixlib";
  };

  outputs = {nixlib, ...} @ inputs:
    nixlib.flake inputs ({...}: {
      systems = ["x86_64-linux"];

      perSystem = {
        self',
        opencode-sandbox',
        kani-flake',
        pkgs,
        libpkgs,
        ...
      }: {
        formatter = pkgs.alejandra;

        packages = rec {
          sandbox = opencode-sandbox'.packages.sandbox.override {
            opencode-sandbox = {
              git.remote.url = "https://github.com/Trantorian1/http.git";
              git.shutdown.pushOnExit = false;
              git.withLocalChanges = true;

              limits.mem = 32768;
              limits.vcpu = 8;

              forwardPorts = [8888];
              env.extend = [devenv];
              opencode.config.mountHomeConfig = true;
            };
          };

          devenv = libpkgs.mkEnv {
            packages = with pkgs; [
              cargo-bolero

              kani-flake'.packages.kani
              (kani-flake'.packages.rust-bin.override {
                extensions = [
                  "rust-analyzer"
                  "rust-src"
                  "rustfmt"
                ];
              })
            ];
          };
        };

        devShells = {
          default = pkgs.mkShell {
            packages = [self'.packages.devenv];
          };
        };
      };
    });
}

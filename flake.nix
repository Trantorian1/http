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

    # `opencode` is a `bun build --compile` executable whose PT_LOAD program
    # headers are not sorted by address. Linux tolerates this but gVisor does
    # not: execve fails past the point of no return and the process silently
    # exits with 255. Re-wrap the upstream build with its headers sorted.
    opencode = pkgs.runCommand "opencode-${pkgs.opencode.version}" {
      nativeBuildInputs = with pkgs; [python3 makeBinaryWrapper];
      inherit (pkgs.opencode) meta;
    } ''
      install -Dm755 ${pkgs.opencode}/bin/.opencode-wrapped $out/bin/opencode
      python3 ${./nix/sort-pt-load.py} $out/bin/opencode
      ln -s ${pkgs.opencode}/share $out/share

      # Same wrapper as nixpkgs
      wrapProgram $out/bin/opencode \
        --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.ripgrep]} \
        --set OPENCODE_DISABLE_AUTOUPDATE true
    '';
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

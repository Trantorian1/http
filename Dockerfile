FROM nixos/nix

RUN echo "experimental-features = nix-command flakes" > /etc/nix/nix.conf
COPY flake.nix flake.lock /src/
COPY nix /src/nix
RUN nix profile install /src

ENV HOME=/root
COPY nix/AGENTS.md /root/.config/opencode/AGENTS.md
RUN nix registry add nixpkgs "path:$(nix eval --raw --impure --expr '(builtins.getFlake "path:/src").inputs.nixpkgs.outPath')"

WORKDIR /work

ENTRYPOINT ["opencode"]

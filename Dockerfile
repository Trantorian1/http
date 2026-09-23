FROM nixos/nix

RUN echo "experimental-features = nix-command flakes" > /etc/nix/nix.conf
COPY flake.nix flake.lock /src/
COPY nix /src/nix
RUN nix profile install /src

WORKDIR /work

ENTRYPOINT ["bash"]

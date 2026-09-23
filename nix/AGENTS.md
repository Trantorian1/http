# Environment

You are running inside a sandboxed container, not on the user's machine. The
project is mounted at /work; anything outside it is lost when the container stops.

Nix is available (flakes enabled). The project's tools are already on PATH.
When you need a tool that isn't installed, use Nix, never apt, pip install,
cargo install, or curl | sh:

- One command: `nix run nixpkgs#<pkg> -- <args>`
- Several:     `nix shell nixpkgs#<pkg> nixpkgs#<pkg2> -c <cmd>`

`nixpkgs` is pinned to the project's flake.lock, so versions match the dev
environment. Guess attribute names (`jq`, `hyperfine`, `gdb`) rather than
running `nix search`, which is slow and memory-hungry. Don't edit flake.nix to
get a tool for yourself; suggest it to the user if it should be permanent.

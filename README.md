# Keys Tools

Small native terminal tools with one job each:

- `fetch` shows a fast system and CPU overview with styled, plain, or JSON output.
- `games` lists and launches terminal games already installed on the machine.

The binaries share no third-party runtime dependencies and never download or
install software.

## Build

Rust 1.97.1 is pinned for builds.

```sh
cargo build --release --locked
./target/release/fetch
./target/release/games list
```

Useful non-interactive forms:

```sh
fetch --plain
fetch --json
games list --json
games info 2048
games run 2048
```

`NO_COLOR=1` disables decoration. `games run` executes the resolved program
directly, never through a shell.

## Game packs

`games` reads optional `*.game` files from directories in `TOOLS_GAME_PATH`
and from the user data directory (`$XDG_DATA_HOME/keys-tools/games`, or
`~/.local/share/keys-tools/games`). A manifest is five bounded, single-line
fields:

```ini
name=maze
command=maze
summary=Small terminal maze
source=https://example.com/maze
license=MIT
```

Names cannot shadow bundled entries. Commands contain one executable and no
shell syntax; arguments are supplied explicitly to `games run`.

## Scope

`fetch` has detailed native collection on macOS and Linux and a smaller
environment-backed view on Windows. It is intentionally not a configurable
replacement for every Fastfetch module. See [research](docs/research.md) and
[benchmarks](docs/benchmarks.md) for the measured comparison and limits.

The PyPI/maturin metadata is preparation only. No package has been published.

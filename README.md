# Keys Tools

Small native terminal tools with one job each:

- `fetch` shows a fast system and CPU overview with styled, plain, or JSON output.
- `games` is an original native terminal arcade with eleven complete game modes.

The binaries never download or install software. `games` uses Crossterm for
raw keyboard input and terminal restoration; no upstream game code, assets, or
runtime is bundled.

## Install

After the first release is published, install both commands from its native
PyPI wheel:

```sh
uv tool install keys-tools
```

The release workflow builds wheels for Linux x86_64, macOS ARM64 and x86_64,
and Windows x86_64. It attaches the same verified files and checksums to the
corresponding GitHub release.

## Build

Rust 1.97.1 is pinned for builds.

```sh
cargo build --release --locked
./target/release/fetch
./target/release/games
```

Useful non-interactive forms:

```sh
fetch --plain
fetch --json
games list --json
games info merge
games run merge --seed 1
```

`NO_COLOR=1` disables decoration. Interactive play needs a real terminal and
restores raw mode, cursor state, wrapping, and the main screen when it exits.

## Native arcade

`games` contains eleven clean-room modes: `delve`, `orbit`, `serpent`,
`merge`, `vector`, `cards`, `seedling`, `sprint`, `keybed`, `glyphs`, and
`scout`. They use an original implementation, maps, words, prose, and ASCII art
informed only by the high-level behavior of the research projects. Sessions
are finite and seeded; v1 has no network, audio, persistence, plugins, external
executables, or copied assets.

## Scope

`fetch` has detailed native collection on macOS and Linux and a smaller
environment-backed view on Windows. It is intentionally not a configurable
replacement for every Fastfetch module. See [research](docs/research.md) and
[fetch benchmarks](benches/fetch.md) for the measured comparison and limits.
The [games benchmark](benches/games.md) measures the native arcade's owned
non-interactive path and binary size.

## Project

- [Changelog](docs/CHANGELOG.md)
- [Contributing](docs/CONTRIBUTING.md)
- [Security policy](docs/SECURITY.md)
- [Code of Conduct](docs/CODE_OF_CONDUCT.md)
- [MIT license](LICENSE)

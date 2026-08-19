# Keys Tools

Small native terminal tools with one job each:

- `apps` reads text, explores CSV/TSV, presents Markdown, paints text canvases, and runs timers.
- `fetch` shows a fast system and CPU overview with styled, plain, or JSON output.
- `games` is a native clean-room terminal arcade with eleven complete game modes.
- `lazybox` is one direct dashboard for Docker containers, Apple containers, and Slurm jobs.
- `science` explores planetary orbits, VCD waveforms, and PLY or `.splat` point clouds offline.
- `screensaver` animates six owned terminal scenes, including local Git history and GIF files.

The binaries never download or install software. `games` and `lazybox` use
Crossterm for raw keyboard input and terminal restoration; no upstream TUI,
game code, or assets are bundled.

## Install

After the first release is published, install all commands from its native
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
./target/release/apps
./target/release/fetch
./target/release/games
./target/release/lazybox
./target/release/science
./target/release/screensaver
```

Useful non-interactive forms:

```sh
apps snapshot read notes.md --find TODO --json
apps snapshot table data.csv --limit 25 --plain
apps snapshot slides talk.md --page 2
apps snapshot timer --seconds 1500 --elapsed 60 --json
fetch --plain
fetch --json
games list --json
games info merge
games run merge --seed 1
lazybox snapshot --backend auto --json
lazybox open --backend docker
science snapshot orbit --json
science open wave capture.vcd
science open cloud scan.ply
screensaver snapshot rain --seed 1 --json
screensaver open git .
screensaver open gif animation.gif
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

## Unified operations

`lazybox` auto-detects a responding Docker, Apple Container, or Slurm backend,
then renders its own consistent dashboard. It calls only the installed native
CLI for inventory, details, bounded container logs, and confirmed start, stop,
restart, or cancellation actions. It never invokes Lazydocker, Lazyslurm, or
Lazycontainer. Plain and JSON snapshots make the same inventory usable in
scripts without entering raw terminal mode. Backend calls stop after 30 seconds
and capture at most 8 MiB of combined output.

## Offline science

`science` calculates approximate heliocentric planet positions locally from the
[JPL Solar System Dynamics 1800-2050 model](https://ssd.jpl.nasa.gov/planets/approx_pos.html).
Its event-driven views also parse bounded VCD logic traces and stream ASCII or
little-endian binary PLY and 32-byte `.splat` point records in one pass.
Consecutive repeated logic states are discarded during parsing while distinct
multi-bit values are preserved, and
large clouds use a deterministic sample of at most 60,000 points for drawing
without weakening full-file bounds and centroid statistics. No source asset,
network service, serial driver, subprocess, or upstream executable is bundled.

## Native screensaver

`screensaver` owns four generated scenes—digital rain, wrapping pipes, a frog
pond, and offline weather—plus a bounded Git-history view and local GIF87a/89a
playback. Generated snapshots are deterministic by seed. Git mode reads one
64-commit batch through the installed `git` command with the same timeout and
output limits as `lazybox`; GIF mode contains a bounded block parser and
fixed-table LZW decoder capped at 64 MiB input, 512 frames, and 16 million
retained terminal-render pixels, with at most 128 million decoded source
pixels. It does not use Pillow, ffmpeg, a browser, a weather API, a
media cache, or copied upstream art.

Interactive output updates only changed terminal rows, supports resize and
`NO_COLOR`, and can be slowed to one frame per second with `--fps 1`. Plain and
JSON snapshots provide a non-animated alternative.

## Native apps

`apps` owns five local workflows in one binary: bounded UTF-8 reading and
search, quoted CSV/TSV parsing, Markdown slide navigation, text-canvas painting,
and a monotonic countdown timer. Every file input is opened once and capped at
32 MiB. Tables are additionally capped at 100,000 rows, 256 columns, and two
million cells. Paint output uses a completed temporary file and an atomic
no-overwrite hard link, so it cannot replace an existing file.

The binary does not bundle or launch MuPDF, Firefox, a browser engine, a
Wayland compositor, a PTY server, TLS, weather/speed services, plugins, or
copied upstream code and assets. Pipe already-fetched text through `-` when a
local snapshot is enough.

## Scope

`fetch` has detailed native collection on macOS and Linux and a smaller
environment-backed view on Windows. It is intentionally not a configurable
replacement for every Fastfetch module. See [research](docs/research.md) and
[fetch benchmarks](benches/fetch.md) for the measured comparison and limits.
The [games benchmark](benches/games.md) measures the native arcade's owned
non-interactive path and binary size. The [lazybox benchmark](benches/lazybox.md)
measures native CLI startup and package size without pretending backend latency
belongs to the dashboard.
The [science benchmark](benches/science.md) measures an offline orbit snapshot
and binary size; file parsing latency remains input-dependent.
The [screensaver benchmark](benches/screensaver.md) measures one deterministic
generated snapshot and binary size; Git and GIF latency remains input-dependent.
The [apps benchmark](benches/apps.md) measures one deterministic timer snapshot
and binary size; file parsing and interactive latency remain input-dependent.

## Project

- [Changelog](docs/CHANGELOG.md)
- [Contributing](docs/CONTRIBUTING.md)
- [Security policy](docs/SECURITY.md)
- [Code of Conduct](docs/CODE_OF_CONDUCT.md)
- [MIT license](LICENSE)

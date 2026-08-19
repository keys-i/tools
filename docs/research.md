# Terminal tool research

Reviewed 2026-08-12 through 2026-08-17 from the requested repositories. The
local `research/` directory contains shallow clones at the revisions below and
is intentionally ignored by Git. Category and license describe the upstream
project; no source or assets are copied into Keys Tools.

| Project | Revision | Category | Runtime | License |
| --- | --- | --- | --- | --- |
| [cpufetch](https://github.com/Dr-Noob/cpufetch) | `b1f20cfec6ff` | CPU information | C | GPL-2.0 |
| [macchina](https://github.com/Macchina-CLI/macchina) | `c049088c20ed` | system information | Rust | MIT |
| [fastfetch](https://github.com/fastfetch-cli/fastfetch) | `c4695b8cf10f` | system/hardware information | C | MIT |
| [lazyslurm](https://github.com/hill/lazyslurm) | `0206b9e69dc1` | Slurm TUI | Rust | MIT |
| [browsh](https://github.com/browsh-org/browsh) | `499ef386d45c` | text browser | JavaScript/Go | LGPL-2.1 |
| [cloudflare-speed-cli](https://github.com/kavehtehrani/cloudflare-speed-cli) | `89c2b2afa064` | speed test | Rust | GPL-3.0 |
| [astroterm](https://github.com/da-luce/astroterm) | `5c571959dbd7` | planetarium | C | MIT |
| [gitlogue](https://github.com/unhappychoice/gitlogue) | `1e59c4feafc3` | Git animation | Rust | ISC |
| [neo](https://github.com/st3w/neo) | `6ba93ac630b0` | screensaver | C++ | GPL-3.0 |
| [rxpipes](https://github.com/inunix3/rxpipes) | `d99c35f2d1d5` | screensaver | Rust | MIT |
| [weathr](https://github.com/veirt/weathr) | `7d403a0c4039` | weather TUI | Rust | GPL-3.0 |
| [pond](https://gitlab.com/alice-lefebvre/pond) | `6f7620940d89` | simulation | C | GPL-3.0-or-later |
| [tock](https://github.com/kriuchkov/tock) | `fd3be28390cb` | time tracking | Go | GPL-3.0 |
| [tui-slides](https://github.com/Chleba/tui-slides) | `44facf92caea` | presentations | Rust | Apache-2.0 |
| [tuios](https://github.com/Gaurav-Gosain/tuios) | `62cd20bdb3f6` | multiplexer | Go | MIT |
| [tuihub](https://github.com/ashis0013/tuihub) | `e53dcb1faab4` | utility hub | Go | MIT |
| [tuiserial](https://github.com/Horldsence/tuiserial) | `b0f507d53fd1` | serial debugger | Rust | MIT |
| [VisiData](https://github.com/saulpw/visidata) | `ea9eecbc5d4b` | data exploration | Python | GPL-3.0 |
| [upiano](https://github.com/eliasdorneles/upiano) | `03cd1e804821` | piano | Python | MIT |
| [tortuise](https://github.com/buildoak/tortuise) | `d114fff4882c` | Gaussian splats | Rust | MIT |
| [Textual Paint](https://github.com/1j01/textual-paint) | `d61de649a6a3` | paint | Python | MIT |
| [tdf](https://github.com/itsjunetime/tdf) | `c2117daaf5f0` | PDF viewer | Rust | AGPL-3.0 |
| [term.everything](https://github.com/mmulet/term.everything) | `c8e04be0ae9a` | GUI-to-terminal bridge | Go | AGPL-3.0 |
| [wttr.in](https://github.com/chubin/wttr.in) | `c807a08bd172` | weather service | Go | Apache-2.0 |
| [tttui](https://github.com/reidoboss/tttui) | `2b74240ecc87` | typing game | Rust | MIT |
| [digisurf](https://github.com/SeanMcLoughlin/digisurf) | `dde47dcbf774` | waveform viewer | Rust | MIT |
| [gif-for-cli](https://github.com/google/gif-for-cli) | `31f8aa2d617d` | GIF renderer | Python | Apache-2.0; archived |
| [botany](https://github.com/jifunks/botany) | `2802121ed826` | virtual plant | Python | ISC |
| [BalatroTUI](https://github.com/Passeriform/BalatroTUI) | `01bbb9ac1b62` | game | Rust | GPL-3.0 |
| [BrogueCE](https://github.com/tmewett/BrogueCE) | `7f52dd93b7fa` | game | C | AGPL-3.0 |
| [raku-wordle](https://github.com/m-dango/raku-wordle) | `b32345f741a1` | game | Raku | Artistic-2.0 |
| [pokete](https://github.com/lxgr-linux/pokete) | `564d6f320a67` | game | Python | GPL-3.0 |
| [rebels-in-the-sky](https://github.com/ricott1/rebels-in-the-sky) | `c83e161b0403` | game | Rust | GPL-3.0 |
| [snake](https://github.com/wick3dr0se/snake) | `3ea304217fc5` | game | Bash | GPL-3.0 |
| [tui-2048](https://github.com/ps06756/tui-2048) | `44a9ff02e928` | game | Python | MIT |
| [awk-raycaster](https://github.com/TheMozg/awk-raycaster) | `ac7f1b03554c` | game/demo | gawk | MIT |
| [lazydocker](https://github.com/jesseduffield/lazydocker) | `7e7aadc2071d` | container TUI | Go | MIT |
| [lazycontainer](https://github.com/andreybleme/lazycontainer) | `42e140b69fcf` | Apple Containers TUI | Go | MIT |
| [ls-horizons](https://github.com/litescript/ls-horizons) | `3ad149902caf` | DSN visualizer | Go | Apache-2.0 |

## Decisions

- `fetch` compares only with cpufetch, macchina, and fastfetch. They are the
  three requested tools with overlapping CPU/system-information output.
- `games` is a clean-room arcade informed only by observable genres and
  high-level behavior. It uses an original implementation, maps, text, words,
  and ASCII assets; no upstream source, data, branding, save format, or executable is
  copied, linked, or launched.
- `lazybox` owns a small dashboard and directly calls the installed `docker`,
  Apple `container`, or Slurm commands. It does not launch or embed Lazydocker,
  Lazycontainer, or Lazyslurm. One row model replaces their unrelated UI and
  dependency stacks; backend-specific operations remain direct `match` arms,
  not a plugin or trait layer.
- `science` consolidates the overlapping offline workloads into three owned
  views: [JPL approximate planetary positions](https://ssd.jpl.nasa.gov/planets/approx_pos.html),
  bounded VCD logic traces, and streaming ASCII or binary little-endian
  PLY/`.splat` point clouds. `tuihub`'s todo list duplicates no science workload,
  while live DSN, plugins, and serial-port control would add network or driver
  stacks, so they are excluded from this offline binary.
- `screensaver` owns four generated scenes plus bounded local Git-history and
  GIF87a/89a views. Its Git path uses the installed `git` command's
  [documented bounded pretty format](https://git-scm.com/docs/pretty-formats.html);
  its GIF parser follows the
  [GIF89a block and LZW specification](https://www.w3.org/Graphics/GIF/spec-gif89a.txt).
  It does not copy upstream art or code, and it excludes live weather,
  geolocation, Tenor, URLs, videos, exports, plugins, and caches because those
  require unrelated privacy, TLS, codec, persistence, or subprocess surfaces.
- Crossterm is the only runtime dependency and owns cross-platform raw input,
  resize events, alternate-screen entry, and terminal restoration. Ratatui is
  excluded because these fixed layouts do not need a widget or layout engine.
- PyPI distribution uses [maturin binary bindings](https://www.maturin.rs/bindings.html),
  which put native binaries on the environment `PATH`. PyO3 is excluded because
  the commands expose no Python API; native wheels are required per
  OS/architecture. [`uv tool install`](https://docs.astral.sh/uv/concepts/tools/)
  then exposes all commands from an isolated tool environment.

# Benchmarks

Measured 2026-08-13 on an Apple M3 Pro (12 cores, 36 GiB), macOS 26.5.2
arm64, rustc 1.97.1, AppleClang 21, and Hyperfine 1.20.0. Each result uses 50
warmups and 500 measured launches. Release binaries were built from the pinned
revisions in [research](research.md), on the same host, immediately before the
run.

| Command | Mean ± standard deviation | Median | Binary size |
| --- | ---: | ---: | ---: |
| `fetch --plain` | 1.438 ± 0.060 ms | 1.436 ms | 336,032 B |
| Fastfetch, eight selected modules and no logo | 4.878 ± 0.439 ms | 4.752 ms | 1,615,608 B |
| Macchina, seven selected readouts | 9.038 ± 0.584 ms | 9.052 ms | 1,677,856 B |

The selected competitor fields cover the common host, OS, kernel, CPU, memory,
shell, and terminal categories. They are not identical: `fetch` additionally
prints architecture, CPU topology, and physical/logical core counts. For this
bounded overview, `fetch` used 70.5% less mean launch time than Fastfetch and
84.1% less than Macchina. Its binary was 79.2% and 80.0% smaller, respectively.

The requested combined workflow was also measured through the same shell:

| Workflow | Mean ± standard deviation | Median |
| --- | ---: | ---: |
| `fetch --plain` | 1.441 ± 0.169 ms | 1.456 ms |
| selected Fastfetch modules, then `cpufetch --logo-short` | 7.604 ± 0.529 ms | 7.423 ms |

The single command used 81.0% less mean launch time (5.28× faster) and one
336,032-byte binary instead of 3,472,592 bytes of binaries. The outputs are not
feature-identical: CPUfetch reports microarchitecture, frequency, instruction
features, and estimated peak performance; `fetch` reports native topology and
L2 cache groups. Fastfetch remains much more configurable and supports many
more modules. In a separate 1,000-sample direct comparison, CPUfetch averaged
1.368 ± 0.075 ms versus `fetch` at 1.472 ± 0.112 ms (7.1% lower). That narrower
tool remains faster for CPU-only output.

To reproduce the measurements from the ignored shallow clones, install C/C++
build tools, CMake, Rust 1.97.1, and Hyperfine 1.20.0, then build the four
binaries at the pinned revisions:

```sh
cargo build --release --locked
cmake -S research/fastfetch -B research/fastfetch/build -DCMAKE_BUILD_TYPE=Release
cmake --build research/fastfetch/build --config Release
cargo build --release --manifest-path research/macchina/Cargo.toml
make -C research/cpufetch
tests/benchmark.sh
```

Set `FETCH`, `FASTFETCH`, `MACCHINA`, and `CPUFETCH` when binaries live outside
their documented defaults. Results are written below ignored `target/benchmarks`.
The system and combined tables used the default `RUNS=500`; the separately
reported CPU-only comparison used `RUNS=1000 tests/benchmark.sh` and its
`target/benchmarks/cpu.json` result. Build flags and available system libraries
can change competitor features and timings, so compare results only from the
same host and build session.

# Science benchmark

The benchmark measures `science snapshot orbit --plain`, a deterministic-size
offline calculation with no file, network, subprocess, or terminal dependency.
VCD and point-cloud latency depends on input size and is therefore not used as
a general startup claim.

Measured 2026-08-17 on Darwin 25.5.0 ARM64 with Rust 1.97.1 and Hyperfine
1.20.0, using 100 runs after 20 warmups:

| Command | Mean ± standard deviation | Range | Stripped binary |
| --- | ---: | ---: | ---: |
| `science snapshot orbit --plain` | 1.5 ± 0.1 ms | 1.4-1.9 ms | 455,424 bytes |

This is an owned startup measurement, not a claim that an orbit snapshot is
equivalent to any upstream interactive workload.

The macOS ARM64 wheel is 875,111 bytes for all four binaries, package metadata,
project and dependency licenses, and the CycloneDX SBOM.

Run the same benchmark used by CI:

```sh
cargo build --release --locked
RUNS=100 WARMUP=20 bash tools/scripts/benchmarks.sh
```

Hyperfine JSON, Markdown, and stripped binary sizes are written below
`target/benchmarks/`. Record a same-host fixture, byte size, signal or point
count, median, and spread before making a parser-performance claim.

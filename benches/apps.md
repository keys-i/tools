# Apps benchmark

The benchmark measures `apps snapshot timer --seconds 1500 --elapsed 60
--plain`, a deterministic local formatting path with no file, terminal,
network, cache, subprocess, or clock dependency. File and interactive latency
depends on input size and is not used as a general startup claim.

Measured 2026-08-18 on Darwin 25.5.0 ARM64 with Rust 1.97.1 and Hyperfine
1.20.0, using 100 runs after 20 warmups:

| Command | Mean ± standard deviation | Range | Stripped binary |
| --- | ---: | ---: | ---: |
| `apps snapshot timer --seconds 1500 --elapsed 60 --plain` | 1.6 ± 0.1 ms | 1.4–1.9 ms | 438,752 bytes |

This is an owned startup measurement, not a claim that a timer snapshot is
equivalent to an upstream PDF renderer, browser, compositor, or network test.

The macOS ARM64 wheel is 1,329,747 bytes for all six binaries, package
metadata, project and dependency licenses, and the CycloneDX SBOM.

Run the same benchmark used by CI:

```sh
cargo build --release --locked
RUNS=100 WARMUP=20 bash tools/scripts/benchmarks.sh
```

Hyperfine JSON, Markdown, and stripped binary sizes are written below ignored
`target/benchmarks/`. Record a same-host fixture, byte/row/cell count, median,
and spread before making a parser-performance claim.

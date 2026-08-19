# Screensaver benchmark

The benchmark measures `screensaver snapshot rain --seed 1 --frame 24
--plain`, a deterministic owned render with no terminal, file, network, cache,
or subprocess dependency. Git and GIF latency depends on input size and is not
used as a general startup claim.

Measured 2026-08-17 on Darwin 25.5.0 ARM64 with Rust 1.97.1 and Hyperfine
1.20.0, using 100 runs after 20 warmups:

| Command | Mean ± standard deviation | Range | Stripped binary |
| --- | ---: | ---: | ---: |
| `screensaver snapshot rain --seed 1 --frame 24 --plain` | 1.9 ± 0.2 ms | 1.6–2.5 ms | 439,264 bytes |

This is an owned startup measurement, not a claim that one static rain frame
is equivalent to an upstream interactive workload.

The macOS ARM64 wheel is 1,101,605 bytes for all five binaries, package
metadata, project and dependency licenses, and the CycloneDX SBOM.

Run the same benchmark used by CI:

```sh
cargo build --release --locked
RUNS=100 WARMUP=20 bash tools/scripts/benchmarks.sh
```

Hyperfine JSON, Markdown, and stripped binary sizes are written below ignored
`target/benchmarks/`. Record a same-host fixture, dimensions, frame count,
median, and spread before making a Git- or GIF-performance claim.

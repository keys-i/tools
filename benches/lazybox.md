# Lazybox benchmark

`lazybox --version` measures native process startup without requiring Docker,
Apple Container, or Slurm. Backend snapshot latency belongs mostly to the
selected external CLI and environment, so it is not used to claim that
Lazybox is faster than the three unrelated upstream applications.

Measured 2026-08-17 on Darwin 25.5.0 ARM64 with Rust 1.97.1 and Hyperfine
1.20.0, using 100 runs after 20 warmups:

| Command | Mean ± standard deviation | Stripped binary |
| --- | ---: | ---: |
| `lazybox --version` | 1.5 ± 0.1 ms | 422,384 bytes |

The macOS ARM64 wheel is 625,260 bytes for all three binaries, package
metadata, project and dependency licenses, and the CycloneDX SBOM.

Run 100 measured launches after 20 warmups and record all three packaged tools:

```sh
cargo build --release --locked
RUNS=100 WARMUP=20 bash benches/run.sh
```

Results and stripped binary sizes are written below `target/benchmarks/`.
Use one host and backend fixture for any future end-to-end comparison.

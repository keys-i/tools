# Native arcade benchmark

The benchmark measures the owned non-interactive contract
(`games list --plain`) and the stripped release binary size. Gameplay is
event-driven and terminal-bound; comparing whole sessions with unrelated
upstream games would not be equivalent.

Measured 2026-08-14 on Darwin 25.5.0 ARM64 with Rust 1.97.1 and Hyperfine
1.20.0, using 100 runs after 20 warmups:

| Version | Mean ± standard deviation | Stripped binary |
| --- | ---: | ---: |
| External launcher (`3b618dd`, reference only) | 1.8 ± 0.1 ms | 354,496 bytes |
| Native eleven-game arcade | 1.5 ± 0.1 ms | 438,992 bytes |

The two list commands are not equivalent workloads, so their timing difference
is not a performance claim. The native binary adds 84,496 bytes (about 24%)
for eleven game engines and cross-platform terminal input.
The macOS ARM64 wheel is 410,899 bytes, including both binaries, package
metadata, project and dependency licenses, and CycloneDX SBOM; that is 67,191
bytes (about 20%) over
the prior 343,708-byte wheel.

Run the same benchmark used by CI:

```sh
RUNS=100 WARMUP=20 bash benches/run.sh
```

Hyperfine JSON, Markdown, and binary sizes are written to
`target/benchmarks/`. Hosted-runner timings are diagnostic evidence, not a
stable regression threshold; compare performance claims on the same host.
No claim is made that one original mode is faster or more feature-complete
than an upstream inspiration: they are different games.

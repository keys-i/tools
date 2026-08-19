# Games benchmark

`games` owns only discovery and safe process launch; the selected game owns
its runtime. The repository benchmark therefore measures `games list --plain`
against the built-in catalog with empty extension directories. It does not
misrepresent a downstream game's startup time as launcher performance.

Run the same benchmark used by CI:

```sh
cargo build --release --locked
benches/run.sh
```

Hyperfine JSON, Markdown, and binary sizes are written to
`target/benchmarks/`. Hosted-runner timings are diagnostic evidence, not a
stable regression threshold; compare performance claims on the same host.

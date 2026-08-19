# Contributing

## Before changing code

Use an existing issue or open one that states the problem and observable
acceptance criteria. Report vulnerabilities privately as described in the
[security policy](SECURITY.md).

Research repositories under `research/` are evidence only. Do not copy their
code or assets into Keys Tools without an explicit license-compatible decision.

## Local checks

Rust 1.97.1 is pinned. Before opening a pull request, run:

```sh
cargo test --locked
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

Workflow and shell changes additionally require Actionlint 1.7.12 and
ShellCheck 0.11.0:

```sh
actionlint
shellcheck scripts/*.sh benches/*.sh
```

For a performance change, install Hyperfine 1.20.0 and run
`benches/run.sh` on the same host before and after. Include the command,
sample count, central result, spread, and binary size in the pull request.

## Change rules

- Keep each command focused and preserve stable `--plain` and `--json`
  behavior.
- Prefer the Rust standard library and native operating-system APIs; justify any
  new dependency with maintenance, license, security, and size costs.
- Validate extension manifests and never route user-controlled commands through
  a shell.
- Add the smallest observable test for changed public, invalid, and failure
  behavior.
- Keep human documentation under `docs/` and benchmark material under
  `benches/`; `README.md` and `LICENSE` are the documentation exceptions
  at the repository root.

Use conventional commit subjects such as `feat:`, `fix:`, `docs:`, and
`perf:`. Release-plz derives semantic versions and the changelog from them.
Maintainers publish only from reviewed Release-plz pull requests.

## Pull requests

Keep a pull request to one coherent change, link its issue, describe observable
behavior, and list exact checks. Do not include generated build output or local
`research/` clones. By contributing, you agree that your contribution is
licensed under the repository's [MIT license](../LICENSE) and that you will
follow the [Code of Conduct](CODE_OF_CONDUCT.md).

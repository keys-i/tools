#!/usr/bin/env bash
set -euo pipefail

project=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$project"

runs=${RUNS:-500}
fetch=${FETCH:-target/release/fetch}
fastfetch=${FASTFETCH:-research/fastfetch/build/fastfetch}
macchina=${MACCHINA:-research/macchina/target/release/macchina}
cpufetch=${CPUFETCH:-research/cpufetch/cpufetch}

for command in hyperfine "$fetch" "$fastfetch" "$macchina" "$cpufetch"; do
    command -v "$command" >/dev/null 2>&1 || {
        printf 'benchmark: missing %s\n' "$command" >&2
        printf 'build the pinned research prerequisites listed in benches/fetch.md\n' >&2
        exit 127
    }
done

mkdir -p target/benchmarks

hyperfine --shell=none --warmup 50 --runs "$runs" \
    --export-json target/benchmarks/system.json \
    --command-name fetch "$fetch --plain" \
    --command-name fastfetch \
    "$fastfetch --logo none --structure Host:OS:Kernel:CPU:CPUCache:Memory:Shell:Terminal --pipe" \
    --command-name macchina \
    "/usr/bin/env NO_COLOR=1 $macchina -o host -o operating-system -o kernel -o processor -o memory -o shell -o terminal"

printf -v fetch_command '%q --plain' "$fetch"
printf -v combined_command \
    '%q --logo none --structure Host:OS:Kernel:CPU:CPUCache:Memory:Shell:Terminal --pipe; %q --logo-short' \
    "$fastfetch" "$cpufetch"
hyperfine --warmup 50 --runs "$runs" \
    --export-json target/benchmarks/combined.json \
    --command-name fetch "$fetch_command" \
    --command-name fastfetch+cpufetch "$combined_command"

hyperfine --shell=none --warmup 50 --runs "$runs" \
    --export-json target/benchmarks/cpu.json \
    --command-name fetch "$fetch --plain" \
    --command-name cpufetch "$cpufetch --logo-short"

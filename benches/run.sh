#!/usr/bin/env bash
set -euo pipefail

project=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
runs=${RUNS:-100}
warmup=${WARMUP:-20}
output="$project/target/benchmarks"
fetch="$project/target/release/fetch"
games="$project/target/release/games"
lazybox="$project/target/release/lazybox"
science="$project/target/release/science"
screensaver="$project/target/release/screensaver"
apps="$project/target/release/apps"

command -v hyperfine >/dev/null 2>&1 || {
    echo "benchmarks: install hyperfine 1.20.0" >&2
    exit 127
}
for binary in "$fetch" "$games" "$lazybox" "$science" "$screensaver" "$apps"; do
    [[ -x "$binary" ]] || {
        echo "benchmarks: run cargo build --release --locked first" >&2
        exit 2
    }
done

mkdir -p "$output"

hyperfine --shell=none --warmup "$warmup" --runs "$runs" \
    --export-json "$output/tools.json" \
    --export-markdown "$output/tools.md" \
    --command-name fetch "$fetch --plain" \
    --command-name games "$games list --plain" \
    --command-name lazybox "$lazybox --version" \
    --command-name science "$science snapshot orbit --plain" \
    --command-name screensaver "$screensaver snapshot rain --seed 1 --frame 24 --plain" \
    --command-name apps "$apps snapshot timer --seconds 1500 --elapsed 60 --plain"

fetch_size=$(wc -c < "$fetch" | tr -d ' ')
games_size=$(wc -c < "$games" | tr -d ' ')
lazybox_size=$(wc -c < "$lazybox" | tr -d ' ')
science_size=$(wc -c < "$science" | tr -d ' ')
screensaver_size=$(wc -c < "$screensaver" | tr -d ' ')
apps_size=$(wc -c < "$apps" | tr -d ' ')
{
    echo "## Keys Tools benchmarks"
    echo
    echo "Commit: \`${GITHUB_SHA:-local}\`"
    echo
    echo "Environment: \`$(uname -smr)\`; \`$(rustc --version)\`; \`$(hyperfine --version | head -n 1)\`"
    echo
    cat "$output/tools.md"
    echo
    echo "| Binary | Size (bytes) |"
    echo "| --- | ---: |"
    echo "| fetch | $fetch_size |"
    echo "| games | $games_size |"
    echo "| lazybox | $lazybox_size |"
    echo "| science | $science_size |"
    echo "| screensaver | $screensaver_size |"
    echo "| apps | $apps_size |"
    echo
    echo "Hyperfine $runs runs after $warmup warmups; hosted-runner results are informational."
} > "$output/summary.md"

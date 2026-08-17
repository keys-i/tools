#!/usr/bin/env bash
set -euo pipefail

[[ $# -eq 1 && -f $1 ]] || {
    echo "smoke-wheel: expected exactly one wheel" >&2
    exit 2
}

python_command=python
command -v "$python_command" >/dev/null 2>&1 || python_command=python3
"$python_command" - "$1" <<'PY'
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1]) as wheel:
    notices = [name for name in wheel.namelist() if name.endswith("THIRD_PARTY_LICENSES.md")]
    assert notices, "wheel omits third-party license notices"
    text = wheel.read(notices[0]).decode()
    assert "crossterm 0.29.0" in text and "Apache License" in text
PY

test_dir=$(mktemp -d)
trap 'rm -rf -- "$test_dir"' EXIT

"$python_command" -m venv "$test_dir"
if [[ ${RUNNER_OS:-} == Windows ]]; then
    python_bin="$test_dir/Scripts/python.exe"
    bin_dir="$test_dir/Scripts"
    suffix=.exe
else
    python_bin="$test_dir/bin/python"
    bin_dir="$test_dir/bin"
    suffix=
fi

"$python_bin" -m pip install --disable-pip-version-check --no-cache-dir --no-deps "$1"
"$bin_dir/fetch$suffix" --json
"$bin_dir/games$suffix" list --json
"$bin_dir/lazybox$suffix" --version
"$bin_dir/science$suffix" snapshot orbit --json

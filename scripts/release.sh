#!/usr/bin/env bash
set -euo pipefail

project=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$project"

fail() {
    echo "release: $*" >&2
    exit 2
}

stable_tag() {
    [[ $1 =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

release_branch() {
    [[ $1 =~ ^releaseplz[0-9]+x[0-9]+x[0-9]+$ ]]
}

package_version() {
    cargo metadata --locked --no-deps --format-version 1 |
        jq -r '.packages[] | select(.name == "keys-tools") | .version'
}

validate_wheels() {
    local wheel
    shopt -s nullglob
    wheels=("${DIST_DIR:-dist}"/*.whl)
    [[ ${#wheels[@]} -eq 4 ]] ||
        fail "expected four wheels, found ${#wheels[@]}"
    for wheel in "${wheels[@]}"; do
        [[ ${wheel##*/} == "keys_tools-${VERSION:?}-"*.whl ]] ||
            fail "unexpected wheel ${wheel##*/}"
    done
}

select_tag() {
    local tag
    if [[ -n ${MANUAL_TAG:-} ]]; then
        tag=$MANUAL_TAG
    else
        tag=$(jq -r '.[0].tag // empty' <<< "${RELEASES:-[]}")
    fi
    stable_tag "$tag" || fail "expected a stable vMAJOR.MINOR.PATCH tag"
    printf 'tag=%s\n' "$tag" >> "${GITHUB_OUTPUT:?}"
}

authorize_release() {
    : "${BASE_BRANCH:?}"
    : "${HEAD_BRANCH:?}"
    : "${HEAD_REPOSITORY:?}"
    : "${MERGED:?}"
    : "${REPOSITORY:?}"
    if authorized_release; then
        echo "authorized=true" >> "${GITHUB_OUTPUT:?}"
    else
        echo "authorized=false" >> "${GITHUB_OUTPUT:?}"
    fi
}

authorized_release() {
    [[ ${MERGED:-} == true && ${BASE_BRANCH:-} == forge &&
        ${HEAD_REPOSITORY:-} == "${REPOSITORY:-}" ]] &&
        release_branch "${HEAD_BRANCH:-}"
}

validate_metadata() {
    local version
    stable_tag "${TAG:-}" || fail "expected a stable vMAJOR.MINOR.PATCH tag"
    version=$(package_version)
    [[ $TAG == "v$version" ]] ||
        fail "$TAG does not match Cargo version $version"
    printf 'version=%s\n' "$version" >> "${GITHUB_OUTPUT:?}"
}

write_checksums() {
    validate_wheels
    (cd "${DIST_DIR:-dist}" && sha256sum -- *.whl > SHA256SUMS)
}

reconcile_github() {
    local file name work
    local -a missing=()
    : "${REPOSITORY:?}"
    : "${TAG:?}"
    work=$(mktemp -d)
    trap 'rm -rf -- "$work"' EXIT
    if ! gh release view "$TAG" --json assets --repo "$REPOSITORY" > "$work/assets.json"; then
        gh release create "$TAG" --verify-tag --generate-notes \
            --title "$TAG" --repo "$REPOSITORY"
        gh release view "$TAG" --json assets --repo "$REPOSITORY" > "$work/assets.json"
    fi
    for file in "${DIST_DIR:-dist}"/*.whl "${DIST_DIR:-dist}"/SHA256SUMS; do
        name=${file##*/}
        if jq --exit-status --arg name "$name" \
            '.assets[] | select(.name == $name)' "$work/assets.json" >/dev/null; then
            gh release download "$TAG" --pattern "$name" --dir "$work" --repo "$REPOSITORY"
            cmp -s "$file" "$work/$name" ||
                fail "existing GitHub asset $name has a different digest"
        else
            missing+=("$file")
        fi
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        gh release upload "$TAG" "${missing[@]}" --repo "$REPOSITORY"
    fi
}

verify_pypi() {
    local attempt digest filename
    local ready=false
    validate_wheels
    for attempt in {1..12}; do
        if curl --fail --silent --show-error \
            "https://pypi.org/pypi/keys-tools/$VERSION/json" --output pypi.json &&
            [[ $(jq '[.urls[] | select(.filename | endswith(".whl"))] | length' pypi.json) -eq 4 ]]; then
            ready=true
            break
        fi
        [[ $attempt -eq 12 ]] || sleep 5
    done
    [[ $ready == true ]] || fail "PyPI did not expose all four wheels"
    for wheel in "${wheels[@]}"; do
        filename=${wheel##*/}
        digest=$(sha256sum "$wheel" | cut -d' ' -f1)
        jq --exit-status --arg filename "$filename" --arg digest "$digest" \
            '.urls[] | select(.filename == $filename and .digests.sha256 == $digest)' \
            pypi.json >/dev/null
    done
}

prepare_pr() {
    local branch version
    local -a open_branches=()
    : "${BASE_SHA:?}"
    : "${REPOSITORY:?}"
    release-plz update --config release-plz.toml \
        --repo-url "https://github.com/$REPOSITORY"
    if git diff --quiet; then
        echo "release: no unreleased changes"
        return
    fi
    version=$(package_version)
    [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
        fail "invalid generated version $version"
    branch=releaseplz${version//./x}
    release_branch "$branch" || fail "invalid branch $branch"

    git config user.name github-actions[bot]
    git config user.email 41898282+github-actions[bot]@users.noreply.github.com
    mapfile -t open_branches < <(
        gh pr list --state open --limit 100 --json headRefName \
            --jq '.[] | .headRefName | select(test("^releaseplz[0-9]+x[0-9]+x[0-9]+$"))'
    )
    [[ ${#open_branches[@]} -le 1 ]] || fail "multiple release pull requests are open"
    if [[ ${#open_branches[@]} -eq 1 ]]; then
        branch=${open_branches[0]}
        git restore Cargo.lock Cargo.toml docs/CHANGELOG.md
        git fetch origin "$branch"
        git switch -c "$branch" --track "origin/$branch"
        git merge --no-edit "$BASE_SHA"
        release-plz update --config release-plz.toml \
            --repo-url "https://github.com/$REPOSITORY"
    elif git ls-remote --exit-code --heads origin "$branch" >/dev/null; then
        fail "$branch already exists; reopen its pull request or remove it explicitly"
    else
        git switch -c "$branch"
    fi

    version=$(package_version)
    [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
        fail "invalid generated version $version"

    while IFS= read -r file; do
        case $file in
            Cargo.lock|Cargo.toml|docs/CHANGELOG.md) ;;
            *) fail "unexpected generated file $file" ;;
        esac
    done < <(git diff --name-only)
    if ! git diff --quiet; then
        git add Cargo.lock Cargo.toml docs/CHANGELOG.md
        git commit --no-gpg-sign -m "chore: release v$version"
    fi
    gh auth setup-git
    git push origin "$branch"
    if [[ ${#open_branches[@]} -eq 0 ]]; then
        gh pr create --base forge --head "$branch" \
            --title "chore: release v$version" \
            --body "Release-plz generated the semantic version and changelog. Merge only after required checks pass."
    else
        gh pr edit "$branch" --title "chore: release v$version"
    fi
}

self_test() {
    stable_tag v1.2.3 || exit 1
    if stable_tag 1.2.3 || stable_tag v1.2.3-rc.1; then
        exit 1
    fi
    release_branch releaseplz1x2x3 || exit 1
    if release_branch release123 || release_branch feature123; then
        exit 1
    fi
    MERGED=true BASE_BRANCH=forge HEAD_BRANCH=releaseplz1x2x3 \
        HEAD_REPOSITORY=keys-i/tools REPOSITORY=keys-i/tools \
        authorized_release || exit 1
    if MERGED=false BASE_BRANCH=forge HEAD_BRANCH=releaseplz1x2x3 \
        HEAD_REPOSITORY=keys-i/tools REPOSITORY=keys-i/tools \
        authorized_release; then
        exit 1
    fi
    if MERGED=true BASE_BRANCH=forge HEAD_BRANCH=feature123 \
        HEAD_REPOSITORY=keys-i/tools REPOSITORY=keys-i/tools \
        authorized_release; then
        exit 1
    fi
    if MERGED=true BASE_BRANCH=forge HEAD_BRANCH=releaseplz1x2x3 \
        HEAD_REPOSITORY=someone/tools REPOSITORY=keys-i/tools \
        authorized_release; then
        exit 1
    fi
    if MERGED=true BASE_BRANCH=main HEAD_BRANCH=releaseplz1x2x3 \
        HEAD_REPOSITORY=keys-i/tools REPOSITORY=keys-i/tools \
        authorized_release; then
        exit 1
    fi
    echo "release: self-test passed"
}

case ${1:-} in
    select-tag) select_tag ;;
    authorize-release) authorize_release ;;
    validate-metadata) validate_metadata ;;
    write-checksums) write_checksums ;;
    reconcile-github) reconcile_github ;;
    verify-pypi) verify_pypi ;;
    prepare-pr) prepare_pr ;;
    --self-test) self_test ;;
    *) fail "usage: $0 {select-tag|authorize-release|validate-metadata|write-checksums|reconcile-github|verify-pypi|prepare-pr|--self-test}" ;;
esac

#!/bin/bash
set -euo pipefail

version="$1"
msg="$2"

function get_full_version() {
    local ver="$1"
    if [[ "$ver" == *.* ]]; then
        echo "0.$ver"
    else
        echo "0.$ver.0"
    fi
}

full_version=$(get_full_version "$version")
prev_tag=$(git describe --tags --abbrev=0)
prev_version=$(get_full_version "$prev_tag")
echo "Releasing version: $full_version (previous: $prev_version)"
# cargo install cargo-jump; echo "See https://github.com/taoky/cargo-jump"
cargo jump --always-jump tsumugu --old-tag="$prev_tag" "$full_version"
./scripts/check-release-version.sh "refs/tags/$version"
git commit -a -m "Bump version to $full_version"
git tag "$version" -m "$msg"
echo "Release prepared. Run 'git push' and 'git push --tags' to publish the release."

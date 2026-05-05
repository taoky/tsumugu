#!/bin/bash
set -euo pipefail

ref="${1:-${GITHUB_REF:-}}"
manifest="${TSUMUGU_CLI_MANIFEST:-tsumugu-cli/Cargo.toml}"

if [[ -z "$ref" ]]; then
    echo "No git ref provided; skipping release version check."
    exit 0
fi

if [[ "$ref" != refs/tags/* ]]; then
    echo "Ref $ref is not a tag; skipping release version check."
    exit 0
fi

tag="${ref#refs/tags/}"

if [[ "$tag" == *.* ]]; then
    expected_version="0.$tag"
else
    expected_version="0.$tag.0"
fi

actual_version=$(
    awk '
        $0 == "[package]" { in_package = 1; next }
        /^\[/ && in_package { exit }
        in_package && $1 == "version" && $2 == "=" {
            gsub(/"/, "", $3)
            print $3
            exit
        }
    ' "$manifest"
)

if [[ -z "$actual_version" ]]; then
    echo "Could not read package.version from $manifest" >&2
    exit 1
fi

if [[ "$actual_version" != "$expected_version" ]]; then
    echo "Release tag $tag expects crate version $expected_version, but $manifest has $actual_version." >&2
    echo "Run release.sh or update the crate version before pushing the tag." >&2
    exit 1
fi

echo "Release tag $tag matches crate version $actual_version."

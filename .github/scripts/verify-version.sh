#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
version="$(bash .github/scripts/package-version.sh)"

if [[ -z "$version" ]]; then
  echo "failed to read package version from Cargo.toml" >&2
  exit 1
fi

if ! grep -Eq "^## ${version} - [0-9]{4}-[0-9]{2}-[0-9]{2}$" CHANGELOG.md; then
  echo "CHANGELOG.md is missing an entry for version ${version}" >&2
  exit 1
fi

if [[ -n "$tag" ]]; then
  normalized_tag="${tag#refs/tags/}"
  normalized_tag="${normalized_tag#v}"

  if [[ "$normalized_tag" != "$version" ]]; then
    echo "git tag '${tag}' does not match Cargo.toml version '${version}'" >&2
    exit 1
  fi
fi

echo "$version"

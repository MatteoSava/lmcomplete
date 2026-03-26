#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: extract-changelog.sh <version>}"

awk -v version="$version" '
  $0 ~ ("^## " version " - ") {
    print
    found = 1
    next
  }
  found && /^## / { exit }
  found { print }
  END {
    if (!found) {
      exit 1
    }
  }
' CHANGELOG.md

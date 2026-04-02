#!/usr/bin/env bash
set -euo pipefail

awk '
  /^\[package\]$/ { in_package = 1; next }
  /^\[/ && in_package { exit }
  in_package && /^version = "/ {
    gsub(/^version = "/, "", $0)
    gsub(/"$/, "", $0)
    print $0
    exit
  }
' Cargo.toml

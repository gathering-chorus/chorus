#!/usr/bin/env bash
# build-signed.sh — cargo build + post-build codesign with stable identifier.
#
# Why: cargo build --release emits ad-hoc-signed Mach-O with a rebuild-dependent
# identifier suffix (e.g., chorus_hook_shim-d1da85efc2bf3103). macOS TCC can
# treat each rebuild as a new binary and re-evaluate Accessibility grants,
# which occasionally flips them OFF. Pinning the identifier with
# `codesign --identifier` keeps TCC's identity stable across rebuilds.
#
# Note: this does NOT prevent macOS overnight security re-validation from
# flipping grants independently (see RCA #114). That's a separate problem.
# This script only addresses the rebuild-triggered class.
#
# Usage:
#   build-signed.sh <shortcut>
#   build-signed.sh <crate-dir> <identifier> <binary-name>
#
# Shortcuts:
#   build-signed.sh chorus-hooks    → signs chorus-hook-shim as com.chorus.hook-shim
#   build-signed.sh chorus-inject   → signs chorus-inject as com.chorus.inject
set -euo pipefail

ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

resolve_crate() {
  case "$1" in
    chorus-hooks)  echo "$ROOT/platform/services/chorus-hooks|com.chorus.hook-shim|chorus-hook-shim" ;;
    chorus-inject) echo "$ROOT/platform/services/chorus-inject|com.chorus.inject|chorus-inject" ;;
    *)             echo "" ;;
  esac
}

if [ $# -eq 1 ]; then
  spec="$(resolve_crate "$1")"
  if [ -z "$spec" ]; then
    echo "build-signed: unknown shortcut '$1' (known: chorus-hooks, chorus-inject)" >&2
    exit 2
  fi
  IFS='|' read -r crate_dir identifier binary_name <<< "$spec"
elif [ $# -eq 3 ]; then
  crate_dir="$1"
  identifier="$2"
  binary_name="$3"
else
  echo "Usage: build-signed.sh <chorus-hooks|chorus-inject>" >&2
  echo "   or: build-signed.sh <crate-dir> <identifier> <binary-name>" >&2
  exit 2
fi

if [ ! -d "$crate_dir" ]; then
  echo "build-signed: crate dir not found: $crate_dir" >&2
  exit 1
fi

echo "build-signed: cargo build --release in $crate_dir"
(cd "$crate_dir" && cargo build --release)

binary="$crate_dir/target/release/$binary_name"
if [ ! -f "$binary" ]; then
  echo "build-signed: binary not produced: $binary" >&2
  exit 1
fi

echo "build-signed: codesign --force --sign - --identifier $identifier"
codesign --force --sign - --identifier "$identifier" "$binary"

echo "build-signed: verify"
codesign -dvvv "$binary" 2>&1 | grep -E "^Identifier=" | head -1

echo "build-signed: done"

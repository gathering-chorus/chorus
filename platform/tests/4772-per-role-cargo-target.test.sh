#!/usr/bin/env bash
# @domain: builds
# #4772 — each account builds in its own folder; a shared target/ fails with
# EPERM (negative proof). Runs werk-build's account_target_4772 tests.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT/platform/services/werk-build"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/$(id -un)}" cargo test --test account_target_4772 -q

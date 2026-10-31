#!/usr/bin/env bash
# @domain: tests
# @test-type: unit
# #4824 — borg-prove reads a test AC from main's nightly since the land, and a
# crawl AC from the crawl's row. Negative proofs: a newer fail, a crawl with
# gaps, and no crawl since the land never read as met.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT/platform/services/borg-prove"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/$(id -un)}" cargo test -q

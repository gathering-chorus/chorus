#!/usr/bin/env bash
# @domain: roles — whether each role's Claude session can answer (#4817)
# @test-type: unit
# #4817 — a logged-out Claude session ran turns for two hours unseen
# (2026-10-30 18:05–19:54, wren + silas). The Stop hook reads the role's
# transcript: a trailing run of "authentication_failed" entries is logged out.
# Runs chorus-hooks' session_auth tests, negative proofs included (#3734):
# a logged-in role never alerts; an old failure followed by an answer is
# logged in; a still-logged-out role is not re-alerted; a nudge to a
# logged-in role carries no logged-out note (AC2).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT/platform/services/chorus-hooks"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/$(id -un)}" cargo test --bin chorus-hooks session_auth -q
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target/$(id -un)}" cargo test --lib logged_out_4817 -q

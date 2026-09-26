#!/usr/bin/env bash
# @test-type: integration — reads the live store (read only) and asks, for each role, whether its login is really recorded. UNMEASURED (exit 2) when athena-make does not answer.
#
# #4328 — Jeff 2026-09-26 08:29: "is ur session better now?" This is his
# question as a check: an open session acting as the role, started, seen in the
# last 5 minutes, one live run with one presence and a boot context.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/platform/scripts/session-live-check" "$@"

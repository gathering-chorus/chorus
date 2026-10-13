#!/usr/bin/env bash
# @domain: domains
# @test-type: unit — reads the repo; no services
#
# #4490 / #4478 — today's door (platform/services/athena-make) is frozen to break
# fixes while athena-gen (designing/services/athena-gen) grows. Jeff, 2026-10-10:
# "the mistake i REALLY want to avoid is just ripping into v1 and then hacking our
# way through to v2 inside of v1."
#
# The guard: athena-make's source may not grow past FROZEN_LINES. A real break fix
# that must add lines raises FROZEN_LINES in the SAME commit, with the card that
# justified it on the line below — so every growth is a reviewed, named decision.
#   FROZEN_LINES history:
#     9975  2026-10-10  #4490 baseline (after #4475 PATCH and #4482's link-removal fix)
#     9977  2026-10-11  #4412 the door asks the model for person sign-in clients (with_person_clients, 2 lines)
#     9983  2026-10-12  #4485 Machine rows mint bare IRIs like Product/Domain (entity_subject: 3 comment, 3 test lines)
#    10088  2026-10-13  raised late by #4519 (Wren): three authz/door break fixes landed without
#                       raising it, and main went red. #4517 a create needs Write or Append (+44),
#                       #4183 a Permission write drops the cache (+36), #4499 reopen 4 (+25).
FROZEN_LINES=10088
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
count() { cat "$1"/*.rs 2>/dev/null | wc -l | tr -d ' '; }
check() { # src-dir → exit 1 when it has grown past FROZEN_LINES
  local n; n=$(count "$1")
  [ "$n" -gt 0 ] || { echo "FAIL: no source found at $1 (the guard must never pass on a missing target)"; return 1; }
  if [ "$n" -gt "$FROZEN_LINES" ]; then
    echo "FAIL: athena-make grew to $n lines (frozen at $FROZEN_LINES). v1 takes break fixes only; build features in athena-gen. A real break fix raises FROZEN_LINES here with its card."
    return 1
  fi
  echo "PASS: athena-make $n lines, at or under the freeze ($FROZEN_LINES)"
}
pass=0; fail=0
if check "$ROOT/platform/services/athena-make/src"; then pass=$((pass+1)); else fail=$((fail+1)); fi
# NEGATIVE PROOF: a feature-sized edit (200 new lines) on a copy of the source goes red
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
cp "$ROOT"/platform/services/athena-make/src/*.rs "$T"/
seq 1 200 | sed 's/^/\/\/ feature line /' >> "$T/lib.rs"
if check "$T" >/dev/null; then echo "FAIL NEGATIVE: a 200-line feature on v1 passed the freeze"; fail=$((fail+1)); else echo "PASS NEGATIVE: a 200-line feature on v1 is red"; pass=$((pass+1)); fi
# NEGATIVE PROOF: a missing target is red, never a vacuous pass
if check "$T/nowhere" >/dev/null; then echo "FAIL NEGATIVE: a missing source dir passed"; fail=$((fail+1)); else echo "PASS NEGATIVE: a missing source dir is red"; pass=$((pass+1)); fi
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]

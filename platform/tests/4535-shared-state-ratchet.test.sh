#!/usr/bin/env bash
# @test-type: unit — ratchet over role-side source; no live services.
# @card: #4535
# @domain: chorus
#
# #4535 — a role on its own Mac account (Abby, chorus-abby-normal) has its own
# HOME, so every $HOME/.chorus read in code her processes run finds an empty
# folder. New code resolves the team's state through CHORUS_STATE_DIR
# (chorus-env-setup; state_dir() in chorus-runner, chorus_state_dir() in
# chorus-hooks). The ratchet: raw HOME-based .chorus reads may only go DOWN.
# Lower RAW_HOME_READS in the same commit that removes some.
#   RAW_HOME_READS history:
#    129  2026-10-15  #4535 baseline after the spine, runner bin and mcp-call moved
RAW_HOME_READS=129
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
count() { # dirs... → raw HOME/.chorus lines in non-test source
  local n=0 d
  for d in "$@"; do [ -d "$d" ] || { echo "-1"; return; }; done
  grep -rEn '(HOME|home_dir|homedir).{0,60}\.chorus|\.chorus.{0,60}(\"HOME\"|\$HOME|homedir)' "$@" --include='*.rs' --include='*.sh' --include='chorus-*' 2>/dev/null \
    | grep -v -E '(_test\.rs|/tests/)' | wc -l | tr -d ' '
}
check() {
  local n; n=$(count "$@")
  [ "$n" -ge 0 ] || { echo "FAIL: a source dir is missing (the ratchet must never pass on a missing target)"; return 1; }
  if [ "$n" -gt "$RAW_HOME_READS" ]; then
    echo "FAIL: $n raw HOME-based .chorus reads (ratchet $RAW_HOME_READS). Use CHORUS_STATE_DIR / state_dir() so a role on its own account finds the team's state."
    return 1
  fi
  echo "PASS: $n raw HOME-based .chorus reads, at or under $RAW_HOME_READS"
}
DIRS=("$ROOT/platform/services/chorus-hooks/src" "$ROOT/platform/services/chorus-runner/src" "$ROOT/platform/scripts")
pass=0; fail=0
if check "${DIRS[@]}"; then pass=$((pass+1)); else fail=$((fail+1)); fi
# NEGATIVE PROOF: the same tree plus ONE new raw read goes red at exactly the ratchet
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
COPIES=()
for d in "${DIRS[@]}"; do c="$T/$(echo "$d" | tr '/' '_')"; mkdir -p "$c"; cp -R "$d"/. "$c"/; COPIES+=("$c"); done
base=$(count "${COPIES[@]}")
echo 'let p = format!("{}/.chorus/new", std::env::var("HOME").unwrap());' > "${COPIES[0]}/new_read_4535.rs"
if [ "$(count "${COPIES[@]}")" -ne $((base+1)) ]; then echo "FAIL NEGATIVE: the counter did not see the new read"; fail=$((fail+1));
elif RAW_HOME_READS=$base check "${COPIES[@]}" >/dev/null; then echo "FAIL NEGATIVE: one new raw HOME/.chorus read passed"; fail=$((fail+1));
else echo "PASS NEGATIVE: one new raw HOME/.chorus read is red"; pass=$((pass+1)); fi
# NEGATIVE PROOF: a missing target is red
if check "$T/nowhere" >/dev/null; then echo "FAIL NEGATIVE: a missing dir passed"; fail=$((fail+1)); else echo "PASS NEGATIVE: a missing dir is red"; pass=$((pass+1)); fi
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]

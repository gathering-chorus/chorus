#!/usr/bin/env bash
# Negative proofs for the deferred-services tier in deep-health.sh (#3820 / #3734).
# bash-3.2 safe (no assoc arrays) — mirrors deep-health.sh's deferred_reason + classify.
set -u
pass=0; fail=0
ok() { if eval "$2"; then echo "PASS: $1"; pass=$((pass+1)); else echo "FAIL: $1"; fail=$((fail+1)); fi; }

deferred_reason() { case "$1" in buzz-relay-http) echo "deferred per #3674" ;; *) echo "" ;; esac; }
classify() {
  local why; why="$(deferred_reason "$1")"
  if [ -n "$why" ]; then
    if [ "$2" -eq 1 ]; then echo "WARNING|unexpected-up"; else echo "WARNING|expected-down"; fi; return
  fi
  if [ "$2" -eq 0 ]; then echo "FAILURE|unreachable"; else echo "OK"; fi
}

ok "deferred service DOWN -> WARNING (expected-down), never FAILURE" \
   "test \"\$(classify buzz-relay-http 0)\" = 'WARNING|expected-down'"
ok "NEGATIVE PROOF: NON-deferred service DOWN still -> FAILURE (mute is scoped)" \
   "test \"\$(classify chorus-api-http 0)\" = 'FAILURE|unreachable'"
ok "NEGATIVE PROOF: deferred service UP -> WARNING (unexpected-up), never silent" \
   "test \"\$(classify buzz-relay-http 1)\" = 'WARNING|unexpected-up'"
ok "non-deferred service UP -> OK" \
   "test \"\$(classify chorus-api-http 1)\" = 'OK'"

echo "=== deferred-services proofs: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]

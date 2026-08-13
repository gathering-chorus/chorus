#!/bin/bash
# classify-nightly-reds — #3850. The anti-"it was load" tool.
#
# A red is NOT closed as "load" on a shrug. This reads each nightly failure log
# and classifies by EVIDENCE, so real defects can't hide behind a load story:
#
#   REAL/context  — the failure reads a path that does not exist NOW (a torn-down
#                   werk dir, a missing file). Deterministic: fails at any load.
#                   Load is FALSIFIED for these — they are real defects.
#   NOISE         — known non-failure (npm tsconfig probe: 28-byte nodefail log).
#   VERIFY        — timing/status assertion with no missing-path evidence. Could
#                   be load-fragility OR a real flake; must be RE-RUN to decide.
#                   Never closed as "load" until a re-run under normal load passes
#                   AND the load source + causal link are named (the 3-part proof).
#
# Output is the honest breakdown: X real, Y noise, Z unresolved. "Load" is never
# a bucket — it is a claim that must be earned per-item in VERIFY.
set -uo pipefail
DIR="${1:-$HOME/.chorus/nightly-failures}"
real=0; noise=0; verify=0
REAL_LIST=""; VERIFY_LIST=""

for f in "$DIR"/*.log; do
  [ -f "$f" ] || continue
  base=$(basename "$f" .log)
  sz=$(wc -c < "$f" | tr -d ' ')

  # NOISE: the npm-with-no-tsconfig probe writes a ~28-byte nodefail stub
  if [ "$sz" -le 40 ] && echo "$base" | grep -q '^npm-'; then
    noise=$((noise+1)); continue
  fi

  # REAL/context (harness): the SUITE ITSELF lives inside a werk checkout that is
  # gone now. The nightly globbed test files across ephemeral werks and ran copies
  # that vanished under it. Deterministic, not load — the suite's own world is gone.
  if echo "$base" | grep -q 'chorus-werk_'; then
    werk=$(echo "$base" | sed -E 's/.*chorus-werk_([^_]+(-[0-9]+)?)_(platform|skills|product|roles).*/\1/')
    if [ -n "$werk" ] && [ ! -e "/Users/jeffbridwell/CascadeProjects/chorus-werk/$werk" ]; then
      real=$((real+1)); REAL_LIST="$REAL_LIST\n    $base -> suite ran from TORN-DOWN werk: chorus-werk/$werk"; continue
    fi
  fi

  # REAL/context: pull any absolute path the failure names as missing, test if it exists NOW
  missing_path=$(grep -aoE '(/Users/[^ :"]+|chorus-werk/[^ :"]+)' "$f" 2>/dev/null \
    | grep -aE 'No such|cannot|not found' -A0 2>/dev/null | head -1)
  # broader: any path in a "No such file" / "grep: <path>:" line
  ctx=$(grep -aoE '(No such file|grep: /[^:]+: No such|cannot (find|read)|read /[^:]+: No such)' "$f" 2>/dev/null | head -1)
  gone_path=$(grep -aoE '/(Users|var)/[^ :"]+' "$f" 2>/dev/null \
    | while read -r p; do [ -e "$p" ] || { echo "$p"; break; }; done | head -1)

  if [ -n "$ctx" ] && [ -n "$gone_path" ]; then
    real=$((real+1)); REAL_LIST="$REAL_LIST\n    $base -> reads gone path: $gone_path"; continue
  fi

  verify=$((verify+1)); VERIFY_LIST="$VERIFY_LIST\n    $base"
done

echo "=== nightly red classification (#3850 — evidence, not 'load') ==="
echo "  REAL/context (deterministic, NOT load — reads a path gone now): $real"
echo -e "$REAL_LIST"
echo
echo "  NOISE (npm tsconfig probe, not a failure): $noise"
echo
echo "  VERIFY (needs re-run; 'load' only if re-run passes + 3-part proof): $verify"
echo -e "$VERIFY_LIST"
echo
echo "  --- The honest headline: $real proven real defects, $noise noise, $verify unresolved."
echo "  --- ZERO closed as 'load'. Load is earned per VERIFY item, never a default bucket."

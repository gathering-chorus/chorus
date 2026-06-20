#!/usr/bin/env bash
# #3519 — the land path MUST record the human GO (werk-demo go) under the ACCEPTER
# before the prove step, or jeff_go_recorded finds nothing and re-presents forever.
# Self-accept guard: recorded under ACCEPTER, never ROLE; GO with no accepter refused.
set -uo pipefail
YML="${1:-$(cd "$(dirname "$0")/../../.." && pwd)/.github/workflows/werk.yml}"
[ -f "$YML" ] || YML="$(git rev-parse --show-toplevel)/.github/workflows/werk.yml"
fail=0
demo_block=$(awk '/^      - name: demo$/{f=1} f{print} /rc=\$\?/{if(f) exit}' "$YML")
echo "$demo_block" | grep -qE 'DEPLOY_ROLE="\$\{ACCEPTER\}" werk-demo go' \
  || { echo "FAIL: land path does not record the go under ACCEPTER (werk-demo go)"; fail=1; }
echo "$demo_block" | grep -qE 'ACCEPTER.*refusing|refusing.*DEC-048' \
  || { echo "FAIL: GO with no accepter is not refused (DEC-048 self-accept guard)"; fail=1; }
# the go-record must come BEFORE the prove call
rec_line=$(echo "$demo_block" | grep -n 'werk-demo go' | head -1 | cut -d: -f1)
prove_line=$(echo "$demo_block" | grep -n 'werk-demo "\${CARD_ID}" "\${GO}"' | head -1 | cut -d: -f1)
if [ -n "$rec_line" ] && [ -n "$prove_line" ] && [ "$rec_line" -lt "$prove_line" ]; then :; else
  echo "FAIL: go-record must precede the prove step"; fail=1; fi
[ "$fail" -eq 0 ] && { echo "PASS: land records the human go under accepter, before prove, accepter-required"; exit 0; }
exit 1

#!/usr/bin/env bash
# @test-type: unit
#
# #4228 — NEGATIVE PROOFS for the land's model-deploy verdict.
#
# Wren's #4216 printed "merged + deployed + LIVE, accepted" while the model
# deploy inside it had exited 1 (source-delete-unretired). She then ticked an
# AC against a store that never changed. The spawn is detached and unref'd, so
# nothing read the exit; the only event was athena.trigger.started.
#
# These fixtures put the three states into a fake spine log and run the outcome
# step's logic over it. Per #3734 each one is the VIOLATION, not the absence:
# a child that failed, and a child still running at the deadline.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
WF="$ROOT/.github/workflows/werk.yml"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
ok()  { echo "  ok   $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

# The outcome step's verdict logic, lifted verbatim from werk.yml so the test
# runs what ships. If the block is renamed or removed, sed finds nothing and
# the extraction below is empty — which fails loudly rather than passing.
sed -n '/#4228 — the model deploy runs detached/,/^            if \[ "\$MODEL" = "failed" \]/p' "$WF" \
  | sed '$d' > "$TMP/verdict.sh"
grep -q 'MODEL="unmeasured"' "$TMP/verdict.sh" \
  || { echo "  FAIL could not extract the verdict block from werk.yml"; exit 1; }

verdict() { # <spine-log-contents> -> prints ok|failed|unmeasured
  printf '%s\n' "$1" > "$TMP/spine.log"
  mkdir -p "$TMP/home/.chorus"
  cp "$TMP/spine.log" "$TMP/home/.chorus/chorus.log"
  {
    echo 'seq() { command seq 1 2; }'   # the deadline, compressed to 2 polls
    echo 'sleep() { :; }'
    echo "CARD_ID=${CARD-4216}"
    cat "$TMP/verdict.sh"
    echo 'echo "$MODEL"'
  } > "$TMP/run.sh"
  HOME="$TMP/home" bash "$TMP/run.sh" 2>/dev/null | tail -1
}

STARTED='{"event":"athena.trigger.started","card_id":"4216"}'

# 1. control — no model deploy for this card at all: not unmeasured, just ok
got=$(verdict '{"event":"something.else","card_id":"4216"}')
[ "$got" = "ok" ] && ok "no model deploy → ok" || bad "no model deploy gave '$got'"

# 2. NEGATIVE PROOF — the child FAILED. This is Wren's #4216, exactly.
got=$(verdict "$STARTED
{\"event\":\"athena.deploy.failed\",\"card_id\":\"4216\",\"exit\":\"1\"}")
[ "$got" = "failed" ] && ok "child exit 1 → failed (never LIVE)" \
  || bad "a refused model deploy read as '$got' — the #4216 defect"

# 3. NEGATIVE PROOF — the child never finished. Silence must not read as success.
got=$(verdict "$STARTED")
[ "$got" = "unmeasured" ] && ok "child still running → unmeasured" \
  || bad "an unfinished model deploy read as '$got'"

# 4. the happy path still passes, or the other two prove nothing
got=$(verdict "$STARTED
{\"event\":\"athena.deploy.completed\",\"card_id\":\"4216\",\"exit\":\"0\"}")
[ "$got" = "ok" ] && ok "child exit 0 → ok" || bad "a clean deploy read as '$got'"

# 5. another card's failure is not this card's failure
got=$(verdict "$STARTED
{\"event\":\"athena.deploy.failed\",\"card_id\":\"9999\",\"exit\":\"1\"}")
[ "$got" = "unmeasured" ] && ok "another card's failure is not ours" \
  || bad "cross-card leak: read as '$got'"

# 6. NEGATIVE PROOF — an empty CARD_ID matches nothing, so every poll silently
# passes and a refused deploy reads as ok. Silas found this at the gate: it is
# the same hollow shape the card exists to remove. Drop the guard and this reds.
got=$(CARD="" verdict "$STARTED
{\"event\":\"athena.deploy.failed\",\"card_id\":\"4216\",\"exit\":\"1\"}")
[ "$got" = "unmeasured" ] && ok "empty CARD_ID → unmeasured, never ok" \
  || bad "empty CARD_ID read as '$got' — a refused deploy would be invisible"

echo "=== Results: $pass passed, $fail failed ==="
[ $fail -eq 0 ]

#!/usr/bin/env bats
# @test-type: e2e — each role's write scope against PRODUCTION
# @domain: security — the product domain this suite guards (#4334)
#
# #4022 — Jeff, 2026-10-13: "i dont see why the api test must be bound to agent
# principals." The API quartet walks once (4279); this suite keeps the one thing
# the three per-role walks also proved: what each role may write on the live door.
#
# For each role, the Permission rows (Write, accessTo graph) and each class's
# servedFrom graph give the plan (lib/scope-plan.py): a create on every class
# the rows refuse, which must come back 403 and write nothing, plus one full
# cycle on a class the rows allow. A class where the door and the rows disagree
# is red, named. Wren, 2026-10-13: "two sources disagreeing on who may write is
# the product bug"; let it go red.
#
# Same gate and label as 4279 (nightly or QUARTET_PROD_CONFIRM=yes; QUARTET_PROD=1
# CHORUS_CONTEXT=prod). Proofs without prod: 4022-role-write-scope-proofs.bats.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
RUNNER="$ROOT/platform/tests/4267-all-generated-apis.test.sh"
PLAN="$ROOT/platform/tests/lib/scope-plan.py"
API="${QUARTET_API:-http://localhost:3360}"
ROLES="${SCOPE_ROLES:-kade wren silas}"

gate_reason() {
  if [ "${WERK_TEST_NIGHTLY:-}" != "1" ] && [ "${QUARTET_PROD_CONFIRM:-}" != "yes" ]; then
    echo "not the nightly and no QUARTET_PROD_CONFIRM=yes — this suite writes PRODUCTION on purpose (#4022)"; return
  fi
  [ -x "$RUNNER" ] || { echo "runner missing: $RUNNER"; return; }
  curl -sf --max-time 5 "$API/health" >/dev/null 2>&1 || echo "$API not answering"
}

setup_file() {
  export GATE; GATE="$(gate_reason)"
  [ -z "$GATE" ] || return 0
  local d="$BATS_FILE_TMPDIR"
  curl -sf --max-time 20 "$API/v1/security/permissions?limit=1000" > "$d/perms.json"
  python3 "$PLAN" graphs "$API" > "$d/graphs.json"
  local run_id; run_id="scope-$(date -u +%Y%m%dt%H%M%Sz)"
  for role in $ROLES; do
    python3 "$PLAN" plan "$d/perms.json" "$d/graphs.json" "$role" > "$d/$role.plan"
    API_BASE="$API" CHORUS_CONTEXT=prod QUARTET_PROD=1 QUARTET_RECORD=0 CHORUS_ROLE="$role" \
      QUARTET_RUN_ID="$run_id-$$-$role" QUARTET_CLASSES="$(awk '{print $2}' "$d/$role.plan" | tr '\n' ' ')" \
      bash "$RUNNER" > "$d/$role.out" 2>&1 || true
  done
}

scope_case() { # $1 role
  [ -z "$GATE" ] || skip "$GATE"
  [ -s "$BATS_FILE_TMPDIR/graphs.json" ] && [ -s "$BATS_FILE_TMPDIR/perms.json" ] \
    || { echo "no Permission rows or class list from $API — scope not measurable"; return 1; }
  run python3 "$PLAN" verdict "$BATS_FILE_TMPDIR/$1.plan" "$BATS_FILE_TMPDIR/$1.out"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "kade: the door refuses every class the Permission rows refuse, and lets one allowed write through" { scope_case kade; }
@test "wren: the door refuses every class the Permission rows refuse, and lets one allowed write through" { scope_case wren; }
@test "silas: the door refuses every class the Permission rows refuse, and lets one allowed write through" { scope_case silas; }

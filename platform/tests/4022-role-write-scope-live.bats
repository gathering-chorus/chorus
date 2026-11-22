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
RUNNER="$ROOT/platform/tests/lib/quartet-walk.sh"  # prod gate reads it
PLAN="$ROOT/platform/tests/lib/scope-plan.py"
API="${QUARTET_API:-http://localhost:3360}"
ROLES="${SCOPE_ROLES:-kade wren silas}"

# shellcheck disable=SC1091
source "$ROOT/platform/tests/lib/prod-gate.sh"
gate_reason() { prod_gate_reason "$ROOT" "$RUNNER" "$API"; }

setup_file() {
  export GATE; GATE="$(gate_reason)"
  [ -z "$GATE" ] || return 0
  local d="$BATS_FILE_TMPDIR"
  curl -sf --max-time 20 "$API/v1/security/permissions?limit=1000" > "$d/perms.json"
  python3 "$PLAN" graphs "$API" > "$d/graphs.json"
  python3 "$PLAN" collections "$API" > "$d/paths.txt"
  # #4915 — a door-written class (writeAuthority "door", #4517) is refused whatever the
  # rows grant; without this list Session read "rows append, door refused" for every
  # role, red every night from 11-19. Same list and rule as 4517's live check (#5002 AC6).
  python3 "$PLAN" door-written "$ROOT"/roles/silas/ontology/*.ttl > "$d/door"
  # #4517 AC2 — one fixed, deliberately invalid create body per class: since AC1 the door
  # judges the body only after authz, so 403 = refuses and 422 = allows, and nothing is
  # ever written. Replaces the full create cycle, which went UNMEASURED wherever no served
  # row gave a value a shape's pattern takes.
  for role in $ROLES; do
    python3 "$PLAN" plan "$d/perms.json" "$d/graphs.json" "$role" > "$d/$role.plan"
    : > "$d/$role.out"
    local tok; tok="$("$ROOT/platform/scripts/chorus-identity-token" "$role" 2>/dev/null || true)"
    while read -r _ cls; do
      local path; path="$(awk -v c="$cls" '$1==c{print $2}' "$d/paths.txt")"
      local code; code="$(curl -s -o "$d/body" -w '%{http_code}' --max-time 20 -X POST \
        -H "Authorization: Bearer $tok" -H 'Content-Type: application/json' \
        --data '{"zz-4517-invalid":1}' "$API$path" || true)"
      python3 "$PLAN" classify "$cls" "${tok:+$code}" "$d/body" >> "$d/$role.out"
    done < "$d/$role.plan"
  done
}

scope_case() { # $1 role
  [ -z "$GATE" ] || skip "$GATE"
  [ -s "$BATS_FILE_TMPDIR/graphs.json" ] && [ -s "$BATS_FILE_TMPDIR/perms.json" ] \
    || { echo "no Permission rows or class list from $API — scope not measurable"; return 1; }
  grep -qx Session "$BATS_FILE_TMPDIR/door" || { echo "the shapes name no door-written Session class — the door-written list is unreadable"; return 1; }
  run python3 "$PLAN" verdict "$BATS_FILE_TMPDIR/$1.plan" "$BATS_FILE_TMPDIR/$1.out" "$BATS_FILE_TMPDIR/door"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "kade: the door refuses every class the Permission rows refuse and allows every class they allow" { scope_case kade; }
@test "wren: the door refuses every class the Permission rows refuse and allows every class they allow" { scope_case wren; }
@test "silas: the door refuses every class the Permission rows refuse and allows every class they allow" { scope_case silas; }

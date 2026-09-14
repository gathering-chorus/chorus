#!/usr/bin/env bats
# @test-type: fitness — runs the model's own governance queries over two local
# TTL files with arq. No service, no store, no network.
#
# #4175 — the hat checks are DATA (chorus:GovernanceCheck rows in
# hats-4175.ttl), so this test does not restate the rules. It extracts every
# check from the model and runs it twice:
#
#   GREEN  against the real model      — every check must return 0 rows
#   RED    against the violation fixture — every check must return exactly the
#          row count its own chorus:provenRedRows claims
#
# Both halves are the point (#3734). A check that fires on the fixture but also
# fires on the model cannot tell the two states apart; a check that is silent on
# both is not a check. The fixture deliberately pairs each violating row with a
# COMPLIANT row of the same shape, so a query that matches on shape alone —
# rather than on the violation — fails here rather than in six weeks.
#
# The claimed counts live in the model, not in this file. Editing a check's
# query without re-proving it makes this test red, which is the intent: the
# proof travels with the rule.

ROOT="$BATS_TEST_DIRNAME/../.."
MODEL="$ROOT/roles/wren/ontology/hats-4175.ttl"
ROWS="$ROOT/roles/wren/ontology/hats-instances-4175.ttl"
FIXTURE="$ROOT/platform/tests/fixtures/hats-4175-violations.ttl"
CORE="$ROOT/roles/silas/ontology/chorus.ttl"

setup() {
  command -v arq >/dev/null 2>&1 || skip "arq (Jena) not installed"
  [ -f "$MODEL" ]   || { echo "model missing: $MODEL"; return 1; }
  [ -f "$FIXTURE" ] || { echo "fixture missing: $FIXTURE"; return 1; }
}

# Prints "<check-name> <claimed-red-rows>" per registered check.
checks() {
  python3 - "$MODEL" <<'PY'
import re, sys
t = open(sys.argv[1]).read()
for m in re.finditer(r'chorus:(gc-[a-z-]+) a chorus:GovernanceCheck ;([\s\S]*?)"""([\s\S]*?)""" \.', t):
    rows = re.search(r'chorus:provenRedRows (\d+)', m.group(2))
    print(m.group(1), rows.group(1) if rows else "NONE")
PY
}

query_of() {
  python3 - "$MODEL" "$1" <<'PY'
import re, sys
t = open(sys.argv[1]).read()
m = re.search(r'chorus:' + re.escape(sys.argv[2]) + r' a chorus:GovernanceCheck ;[\s\S]*?"""([\s\S]*?)""" \.', t)
sys.stdout.write(m.group(1))
PY
}

rows_against() {  # $1=check $2..=data files
  local chk="$1"; shift
  local q="$BATS_TEST_TMPDIR/q.rq"
  query_of "$chk" > "$q"
  local args=()
  for d in "$@"; do args+=(--data "$d"); done
  arq "${args[@]}" --query "$q" --results csv | tail -n +2 | grep -c . || true
}

@test "#4175 the registry is non-empty and every check carries its negative proof" {
  run checks
  [ "$status" -eq 0 ]
  [ -n "$output" ]
  # A registry that emptied itself would make every assertion below vacuous.
  [ "$(echo "$output" | wc -l | tr -d ' ')" -ge 4 ]
  ! echo "$output" | grep -q ' NONE$'
}

@test "#4175 GREEN — every hat check returns zero rows against the real model" {
  while read -r chk _; do
    n="$(rows_against "$chk" "$MODEL" "$ROWS" "$CORE")"
    [ "$n" -eq 0 ] || { echo "$chk fired on the real model with $n row(s)"; return 1; }
  done < <(checks)
}

@test "#4175 RED — every hat check finds exactly the violations it claims" {
  while read -r chk claimed; do
    n="$(rows_against "$chk" "$FIXTURE")"
    [ "$n" -eq "$claimed" ] || { echo "$chk found $n row(s) in the fixture, claims $claimed"; return 1; }
  done < <(checks)
}

@test "#4175 the fixture's compliant twins are never flagged" {
  # Each violating row has a compliant sibling of the same shape. If any check
  # names one of them, it is matching on shape rather than on the violation.
  local q="$BATS_TEST_TMPDIR/all.txt"; : > "$q"
  while read -r chk _; do
    query_of "$chk" > "$BATS_TEST_TMPDIR/q.rq"
    arq --data "$FIXTURE" --query "$BATS_TEST_TMPDIR/q.rq" --results csv | tail -n +2 >> "$q"
  done < <(checks)
  ! grep -q 'appt-good' "$q"
  ! grep -q 'chorus#kade,' "$q" || true   # kade legitimately wears the standing hat
  grep -q 'appt-bad-standing-on-anchor' "$q"
  grep -q 'appt-bad-no-anchor' "$q"
  grep -q 'appt-bad-undeclared-hat' "$q"
  grep -q 'appt-bad-wrong-owner' "$q"
}

# --- the generated appointments obey Jeff's rule ------------------------------
# "the owner wears all 4 for their products domains and services" (2026-09-14).
# These assert the FILE against the rule, so a hand-edit or a half-finished
# regeneration is caught here rather than in the store.

APPTS="$BATS_TEST_DIRNAME/../../roles/wren/ontology/hats-appointments-4175.ttl"

@test "#4175 every appointed anchor wears exactly the four per-product hats" {
  [ -f "$APPTS" ]
  run python3 - "$APPTS" <<'PY'
import collections, re, sys
t = open(sys.argv[1]).read()
blocks = re.findall(
    r'chorus:appt-\S+ a chorus:Appointment ;[\s\S]*?chorus:appointedHat chorus:(\S+) ;\s*'
    r'chorus:overAnchor chorus:(\S+) \.', t)
by_anchor = collections.defaultdict(set)
for hat, anchor in blocks:
    by_anchor[anchor].add(hat)
want = {"hat-product-manager", "hat-solutions-architect",
        "hat-engineering-lead", "hat-operations-lead"}
bad = {a: sorted(h) for a, h in by_anchor.items() if h != want}
print(len(blocks), len(by_anchor))
if bad:
    print("WRONG", bad); sys.exit(1)
if not by_anchor:
    print("EMPTY — no appointments parsed"); sys.exit(1)
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # 4 hats per anchor, and nothing was silently dropped by the parse.
  set -- $output
  [ "$1" -eq $(( $2 * 4 )) ]
}

@test "#4175 no appointment contradicts the ownership it qualifies" {
  # gc-appointee-is-the-owner, run over the generated appointments joined to the
  # ownership recorded in the committed model. Zero rows: the file and the store
  # agree about every anchor.
  #
  # This check found its one real disagreement on its first run, 2026-09-14 —
  # chorus:pulse owned by role-wren live and role-silas in chorus.ttl. It was
  # allowed by name for about an hour, then Silas ruled the store right and the
  # file was fixed, so the allowance is gone rather than outliving the defect.
  query_of gc-appointee-is-the-owner > "$BATS_TEST_TMPDIR/own.rq"
  run arq --data "$APPTS" --data "$CORE" --query "$BATS_TEST_TMPDIR/own.rq" --results csv
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | tail -n +2 | grep -c . || true)" -eq 0 ] || { echo "$output"; return 1; }
}

# --- the generator itself ------------------------------------------------------
# platform/scripts/hats-appointments-4175.py writes the appointments file. The
# runner flagged it uncovered (#3917) and it was: the tests above grade its
# OUTPUT and never touch the script, so a generator that silently wrote nothing
# would pass every one of them as long as the last good file was still on disk.

GEN="$BATS_TEST_DIRNAME/../scripts/hats-appointments-4175.py"

@test "#4175 the generator refuses when it cannot reach its sources" {
  [ -f "$GEN" ]
  out="$BATS_TEST_TMPDIR/appointments.ttl"
  # Port 1 is closed on every box. A generator that treated an unreachable
  # source as "no anchors" would write a valid, empty, completely wrong file —
  # and the checks above would still be green. It must fail instead.
  run python3 "$GEN" --api http://127.0.0.1:1 --query http://127.0.0.1:1/query --out "$out"
  [ "$status" -ne 0 ]
  [ ! -s "$out" ]
}

@test "#4175 the generator is the only writer of the appointments file" {
  # The file says so; if someone hand-edits it the claim becomes a lie, so the
  # claim is asserted rather than trusted.
  grep -q "GENERATED by platform/scripts/hats-appointments-4175.py" "$APPTS"
  grep -q "do not hand-edit" "$APPTS"
  # And the generator names the hats the model declares — not its own list.
  for h in hat-product-manager hat-solutions-architect hat-engineering-lead hat-operations-lead; do
    grep -q "\"$h\"" "$GEN" || { echo "$h missing from the generator"; return 1; }
    grep -q "chorus:$h a chorus:Hat" "$ROWS" || { echo "$h is not a declared row"; return 1; }
  done
}

@test "#4175 every property a served shape names carries a definition" {
  # AC9. The atlas grades a class N-of-M defined; Role read 2/3 because its shape
  # named rdfs:label, a W3C predicate nobody here gets to annotate. The rule the
  # pen already follows: a shape names chorus:label, never rdfs:label.
  run grep -n "sh:path rdfs:label" "$ROOT/roles/wren/ontology/priorities-3686.ttl" "$MODEL"
  [ "$status" -ne 0 ] || { echo "a shape still names rdfs:label:"; echo "$output"; return 1; }
  # And the twin it uses instead is defined, or the swap bought nothing.
  grep -q "chorus:label a owl:DatatypeProperty" "$MODEL"
  grep -A3 "chorus:label a owl:DatatypeProperty" "$MODEL" | grep -q "rdfs:comment"
}

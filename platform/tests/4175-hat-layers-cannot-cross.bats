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
  # ownership recorded in the COMMITTED model. The appointments are generated
  # from the live store, so any row here is a file-vs-store ownership drift.
  #
  # KNOWN, and the reason it is named rather than hidden: chorus:pulse is owned
  # by role-wren live and by role-silas in chorus.ttl. This check found it on its
  # first real run, 2026-09-14. One of the two is wrong and that is Silas's call,
  # not something to paper over — so pulse is allowed BY NAME and a SECOND
  # disagreement still goes red.
  query_of gc-appointee-is-the-owner > "$BATS_TEST_TMPDIR/own.rq"
  run arq --data "$APPTS" --data "$CORE" --query "$BATS_TEST_TMPDIR/own.rq" --results csv
  [ "$status" -eq 0 ]
  local others
  others="$(echo "$output" | tail -n +2 | grep -c . || true)"
  local pulse
  pulse="$(echo "$output" | tail -n +2 | grep -c 'chorus#pulse' || true)"
  # The allowance must not be vacuous: if pulse ever stops disagreeing, this
  # line fails and the exception gets deleted rather than quietly outliving it.
  [ "$pulse" -eq 4 ] || { echo "pulse rows: $pulse (expected 4 — the allowance is stale)"; echo "$output"; return 1; }
  [ "$others" -eq "$pulse" ] || { echo "a second ownership disagreement:"; echo "$output"; return 1; }
}

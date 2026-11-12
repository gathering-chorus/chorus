#!/usr/bin/env bats
# @test-type: contract — reads the model source, the retirement ledger and the tests that read checks; no store.
# @domain: policies
# @card: #4960
# @owner: silas
#
# #4960 AC2 — GovernanceCheck is folded into Policy (#4959's shape) and retired: no model
# file declares it, the ledger retires its class, its shape and its five predicates, and its
# rows leave the domains graph at the land. The #4579 trap (a retirement deleting the
# CoreShape property nodes every flattened shape shares) is guarded by athena-deploy's own
# retiring_a_shape_keeps_property_nodes_another_shape_shares; GovernanceCheckShape is
# retired through that same path. The tests that read a check now read it from the policy
# plan, so a check keeps one home.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LEDGER="$ROOT/designing/schemas/model-retirements.jsonl"

declared_in() { # $1 = a git tree; prints every ttl that still declares or uses the class
  # -w, not \b: git grep's ERE has no \b, which made this pass on any tree (its negative proof caught it)
  git -C "$1" grep -lwE 'chorus:(GovernanceCheck|GovernanceCheckShape|boundTo|checkQuery|checkSeverity|provenRedOn|provenRedRows)' -- '*.ttl' ':!roles/silas/ontology/vocabulary-generated-4254.ttl' || true
}

@test "no model or row file declares GovernanceCheck or uses its predicates" {
  run declared_in "$ROOT"
  [ -z "$output" ] || { echo "$output"; return 1; }
  run grep -c 'ontology/governance-checks-3846.ttl"),' "$ROOT/platform/services/athena-deploy/src/lib.rs"
  [ "$output" = "0" ] || return 1
}

@test "negative proof: a tree that still declares the class is named" {
  d="$BATS_TEST_TMPDIR/tree"; mkdir -p "$d/roles/x/ontology"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:GovernanceCheck a <http://www.w3.org/2002/07/owl#Class> .\n' > "$d/roles/x/ontology/back.ttl"
  git -C "$d" init -q . && git -C "$d" add -A
  run declared_in "$d"
  [[ "$output" == *"back.ttl"* ]] || { echo "not caught: $output"; return 1; }
}

@test "the ledger retires the class's rows, its shape and its five predicates, and the domain's claim on it" {
  run ledger_check "$LEDGER"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: a ledger missing the shape's retirement is refused" {
  grep -v 'GovernanceCheckShape' "$LEDGER" > "$BATS_TEST_TMPDIR/ledger"
  run ledger_check "$BATS_TEST_TMPDIR/ledger"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"GovernanceCheckShape"* ]] || return 1
}

ledger_check() {
  python3 - "$1" <<'PY'
import json, sys
C = "https://jeffbridwell.com/chorus#"
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
mine = [r for r in rows if r.get("card") == "4960" and r.get("status") == "staged"]
subjects = {r.get("retire_subject") for r in mine}
want = {C + s for s in ("GovernanceCheck", "GovernanceCheckShape", "boundTo", "checkQuery", "checkSeverity", "provenRedOn", "provenRedRows")}
missing = want - subjects
cls = [r for r in mine if r.get("retire_class") == C + "GovernanceCheck" and r.get("graph") == "urn:chorus:domains:domains"]
vocab = [r for r in mine if r.get("object_class") == "GovernanceCheck" and r.get("predicate") == "definesVocabulary"]
print("missing:", sorted(missing), "class:", len(cls), "vocab:", len(vocab))
sys.exit(1 if missing or not cls or not vocab else 0)
PY
}

@test "the tests that read a check read it from the policy plan, not a GovernanceCheck file" {
  run git -C "$ROOT" grep -l 'governance-check-instances.ttl' -- platform ':!platform/tests/4960-governance-check-retired.bats'
  [ -z "$output" ] || { echo "$output"; return 1; }
  [ ! -e "$ROOT/designing/data/governance-check-instances.ttl" ] || return 1
}

@test "negative proof: the domain's claim is retired before the class (athena-deploy refuses a claimed class)" {
  run python3 - "$LEDGER" <<'PY'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1]) if l.strip()]
idx = {("vocab" if r.get("predicate") == "definesVocabulary" else "class" if r.get("retire_class") else ""): i
       for i, r in enumerate(rows) if r.get("card") == "4960"}
print(idx); sys.exit(0 if idx.get("vocab", 10**9) < idx.get("class", -1) else 1)
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

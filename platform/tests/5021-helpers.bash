# @domain: principles
# #5021 — shared setup: today's principles snapshot, before and after this card's changes,
# the shapes the werk deploys, and the complete-domain policy rows as the land leaves them.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
FX="$ROOT/platform/tests/fixtures/5021"
SNAP="$FX/principles-2026-11-20.nq"
PV="$ROOT/platform/scripts/policy-validate"
COUNT="$ROOT/platform/scripts/policy-count"

build_5021() {
  T="$BATS_TEST_TMPDIR"
  python3 "$FX/after.py" "$ROOT" "$SNAP" "$T/after.nq"
  python3 "$FX/after.py" --before "$ROOT" "$SNAP" "$T/before.nq"
  for f in roles/silas/ontology/chorus.ttl roles/silas/ontology/core-shape-4576.ttl \
           roles/silas/ontology/session-model-4302.ttl roles/wren/ontology/principles-3749.ttl; do
    riot --output=nt "$ROOT/$f"
  done > "$T/shapes.nt"
  # the complete-domain rows as the land leaves them: #5020's, with #5021's changes over them
  python3 - "$ROOT" "$T/policies.json" <<'PY'
import json, sys
d = sys.argv[1] + "/designing/domains/policies/"
a, b = json.load(open(d + "policy-checks-5020.json")), json.load(open(d + "policy-checks-5021.json"))
names = {r["name"] for r in b}
# domain-folds-filled reads the live page API; AC4's suite proves it on a stub, the land on prod
rows = [r for r in a if r["name"] not in names] + [r for r in b if r["name"] != "domain-folds-filled"]
json.dump({"data": rows}, open(sys.argv[2], "w"))
PY
  export POLICIES_JSON="$T/policies.json" SHACL_SHAPES="$T/shapes.nt" CHORUS_HOME="$ROOT"
}

# the principles lint over one of the two datasets
lint() {
  POLICY_DATA="$T/$1.nq" SHACL_DATA="$T/$1.nq" "$PV" --domain principles
}

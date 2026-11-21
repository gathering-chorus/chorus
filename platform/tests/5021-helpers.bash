# @domain: principles
# #5021 — shared setup: today's principles snapshot, before and after this card's changes,
# the shapes the werk deploys, and the complete-domain policy rows as the land leaves them.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
FX="$ROOT/platform/tests/fixtures/5021"
# today's snapshot, in four parts so each stays under the reviewers' read limit:
# principles-2026-11-20.part-aa.nq principles-2026-11-20.part-ab.nq
# principles-2026-11-20.part-ac.nq principles-2026-11-20.part-ad.nq
SNAP_PARTS=("$FX/principles-2026-11-20.part-aa.nq" "$FX/principles-2026-11-20.part-ab.nq"
            "$FX/principles-2026-11-20.part-ac.nq" "$FX/principles-2026-11-20.part-ad.nq")
PV="$ROOT/platform/scripts/policy-validate"
COUNT="$ROOT/platform/scripts/policy-count"

build_5021() {
  T="$BATS_TEST_TMPDIR"
  SNAP="$T/snapshot.nq"
  cat "${SNAP_PARTS[@]}" > "$SNAP"
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

# run a crate's own status tests and require each to run and pass (a name that exists but never
# runs, or runs and fails, is red): the store, write and filter cases athena-gen generates
status_tests_pass() {
  local out
  out=$(cd "$ROOT/$1" && nice cargo test --release -- --test-threads=1 status 2>&1) || { echo "$out"; return 1; }
  for t in a_row_serves_status_only_when_its_shape_declares_one a_write_of_status_to_a_class_without_one_is_refused_off_model \
           a_status_filter_on_a_class_without_one_is_refused_by_name; do
    echo "$out" | grep -q "embedded_tests::$t \.\.\. ok" || { echo "not run or not ok: $t"; echo "$out"; return 1; }
  done
}

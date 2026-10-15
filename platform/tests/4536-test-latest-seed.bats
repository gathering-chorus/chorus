#!/usr/bin/env bats
# @test-type: unit — hermetic: Loki and the registry are fixture files through the script's seams; no store, no network
# @domain: tests
#
# #4536 — one seed fills each Test row's latest result from one canonical
# nightly's case events. Only rows whose result differs are written, and a werk
# can never write.

SEED="$BATS_TEST_DIRNAME/../scripts/test-latest-seed"

setup() {
  T="$BATS_TEST_TMPDIR"
  ev() { python3 -c 'import json,sys; print(json.dumps(json.dumps({"event":sys.argv[1],"file":sys.argv[2],"case":sys.argv[3],"reason":sys.argv[4]})))' "$@"; }
  cat > "$T/loki.json" <<EOF
{"data":{"result":[{"stream":{},"values":[
 ["1",$(ev test.case.passed a.bats one "")],
 ["2",$(ev test.case.failed a.bats two "boom")],
 ["3",$(ev test.case.skipped a.bats three "")],
 ["4",$(ev test.case.passed b.test.ts "Suite name four" "")]]}]}}
EOF
  cat > "$T/reg.json" <<'EOF'
{"data":[
 {"filePath":"a.bats","testName":"one","name":"t-one","result":"pass"},
 {"filePath":"a.bats","testName":"two","name":"t-two"},
 {"filePath":"a.bats","testName":"three","name":"t-three"},
 {"filePath":"b.test.ts","testName":"name four","name":"t-four"},
 {"filePath":"c.bats","testName":"never ran","name":"t-five"}]}
EOF
  export TEST_SEED_LOKI_FILE="$T/loki.json" TEST_SEED_REGISTRY_FILE="$T/reg.json"
}

@test "dry run: counts what it would write, writes nothing, names only the rows that differ" {
  run "$SEED" nightly-1-2
  [ "$status" -eq 0 ]
  # one already says pass (unchanged); two, three, four would be written;
  # five never ran, so it becomes unmeasured "did not run" (Silas 08:32)
  [[ "$output" == *"4 case events · 5 Test rows · 4 joined · 4 to write"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *'{"fail": 1, "pass": 1, "unmeasured": 2}'* ]] || { echo "$output"; return 1; }
  [[ "$output" == *'1 of them become unmeasured "did not run in nightly-1-2"'* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"dry run — nothing written"* ]] || return 1
}

@test "a skipped case seeds as unmeasured, the only third value the Test shape takes" {
  run "$SEED" nightly-1-2
  [[ "$output" == *'"unmeasured": 2'* ]] || return 1
}

@test "after the seed, the same run writes nothing (the seed is never paid twice)" {
  python3 - "$T/reg.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.load(open(p))
for r, v in zip(d["data"], ["pass", "fail", "unmeasured", "pass", "unmeasured"]):
    if v: r["result"] = v
json.dump(d, open(p, "w"))
PY
  run "$SEED" nightly-1-2
  [ "$status" -eq 0 ]
  [[ "$output" == *"0 to write"* ]] || { echo "$output"; return 1; }
}

@test "NEGATIVE PROOF: --apply from a werk is refused before anything is read" {
  case "$SEED" in */chorus-werk/*) ;; *) skip "this copy of the script is not in a werk" ;; esac
  run "$SEED" nightly-1-2 --apply
  [ "$status" -eq 3 ]
  [[ "$output" == *"a werk never writes prod's Test rows"* ]] || return 1
}

@test "no case events is exit 2, never an empty seed" {
  echo '{"data":{"result":[]}}' > "$T/loki.json"
  run "$SEED" nightly-1-2
  [ "$status" -eq 2 ]
}

@test "--unseen-only writes only the rows the run never ran" {
  run "$SEED" nightly-1-2 --unseen-only
  [ "$status" -eq 0 ]
  [[ "$output" == *"1 to write {\"unmeasured\": 1}"* ]] || { echo "$output"; return 1; }
}

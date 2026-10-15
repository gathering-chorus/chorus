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

# #4537 — the write token lives 600s; the 10-15 seed wrote 158 rows, then its
# token expired and 9,641 were refused. A 401 mints a new token and retries.
door_and_root() {
  mkdir -p "$T/root/platform/scripts"
  cp "$SEED" "$T/root/platform/scripts/test-latest-seed"
  printf '#!/bin/sh\nn=$(cat "%s/n" 2>/dev/null || echo 0); n=$((n+1)); echo $n > "%s/n"; echo "tok-%s$n"\n' "$T" "$T" "$1" \
    > "$T/root/platform/scripts/chorus-identity-token"
  chmod +x "$T/root/platform/scripts/chorus-identity-token"
  cat > "$T/door.py" <<'PY'
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_PATCH(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0)))
        ok = self.headers.get("Authorization", "") != "Bearer tok-1"
        self.send_response(204 if ok else 401); self.end_headers()
    def log_message(self, *a): pass
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(sys.argv[1], "w").write(str(s.server_address[1])); s.serve_forever()
PY
  python3 "$T/door.py" "$T/port" & DOOR=$!
  while [ ! -s "$T/port" ]; do sleep 0.1; done
  export OWL_API_TESTS="http://127.0.0.1:$(cat "$T/port")/tests?limit=10000"
}

@test "an expired token is minted again and the write retried" {
  door_and_root ""
  run "$T/root/platform/scripts/test-latest-seed" nightly-1-2 --apply
  kill "$DOOR"
  [[ "$output" == *"written 4 · refused 0"* ]] || { echo "$output"; return 1; }
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a token that is never accepted is refused and named, never counted written" {
  door_and_root "never-"
  # every token this stub mints is tok-never-N; make the door refuse them all
  sed -i '' 's/!= "Bearer tok-1"/== "never"/' "$T/door.py" 2>/dev/null || true
  kill "$DOOR"; rm -f "$T/port"
  python3 "$T/door.py" "$T/port" & DOOR=$!
  while [ ! -s "$T/port" ]; do sleep 0.1; done
  export OWL_API_TESTS="http://127.0.0.1:$(cat "$T/port")/tests?limit=10000"
  run "$T/root/platform/scripts/test-latest-seed" nightly-1-2 --apply
  kill "$DOOR"
  [ "$status" -eq 1 ]
  [[ "$output" == *"written 0 · refused 4"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *'"HTTP 401": 4'* ]] || { echo "$output"; return 1; }
}

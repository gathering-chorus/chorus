#!/usr/bin/env bats
# @test-type: integration
# #4185 — the crawler writes, updates and deletes test CASE rows.
#
# Jeff, 2026-09-16: "to me the crawler writes and updates tests graph data."
# #4154 and #4173 said "test file"; the crawler tagged files and wrote nothing
# about the cases inside them, so when #4173 deleted the hydrator the per-case
# registry (7,883 rows the runner selects from) froze at 2026-09-13 01:42.
#
# Proven live against a werk VARIANT, never prod (#4180 pattern): a fixture git
# repo is the tree, the variant's athena-make is the door, principal-crawler is
# the identity. Every proof here runs the real binary end to end and reads the
# rows back through the door. Also carries the file-lifecycle proof #4154's
# suite held for the retired Python walker (a file appears, changes, leaves).

# bash 3.2 (this Mac) never fires errexit on a failing `[[ ]]`, so a `[[` assert
# that is not the LAST line of a test can fail and the test still passes (#4185,
# measured 2026-09-16: `[[ "a" == *"b"* ]]; true` → ok). Every assert here is a
# simple command, which bash 3.2 does honour.
has()   { grep -qF -- "$1" <<<"${2-$output}"; }
lacks() { if grep -qF -- "$1" <<<"${2-$output}"; then echo "unexpected: $1" >&2; return 1; fi; }
eq()    { [ "$1" = "$2" ] || { echo "expected [$2] got [$1]" >&2; return 1; }; }

setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration — RUN_INTEGRATION=true against a werk variant"
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  OWL_URL="${OWL_URL:-}"
  case "$OWL_URL" in ""|*:3360*) skip "refuses to write to the canonical store — point OWL_URL at a werk variant" ;; esac
  BIN="${CHORUS_CRAWL_BIN:-$REPO/platform/services/chorus-crawl/target/release/chorus-crawl}"
  [ -x "$BIN" ] || BIN="$REPO/platform/services/chorus-crawl/target/debug/chorus-crawl"
  [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
  TC="$("$REPO/platform/scripts/chorus-identity-token" crawler 2>/dev/null)"; [ -n "$TC" ]

  # the fixture tree: a git repo with two test files and one code file
  FX="$BATS_TEST_TMPDIR/repo"; mkdir -p "$FX/platform/tests" "$FX/src"
  TAG="fx4185$$"                       # unique per run so parallel suites never share rows
  BATS_F="platform/tests/$TAG-suite.bats"
  TS_F="platform/tests/$TAG-unit.test.ts"
  printf '@test "%s first case" {\n  true\n}\n@test "%s second case" {\n  true\n}\n' "$TAG" "$TAG" > "$FX/$BATS_F"
  printf "it('%s jest case', () => {});\n" "$TAG" > "$FX/$TS_F"
  printf 'fn main() {}\n' > "$FX/src/main.rs"
  git -C "$FX" init -q && git -C "$FX" add -A && git -C "$FX" -c user.name=t -c user.email=t@t commit -q -m fixture

  export CHORUS_ROOT="$FX" CHORUS_OWL_API="$OWL_URL" CHORUS_ROLE=crawler CHORUS_IDENTITY_TOKEN="$TC"
  TESTS_COLL="$(collection Test)"; FILES_COLL="$(collection CodeFile)"
}

# the door's own collection for a class — never a literal path (#4158)
collection() {
  curl -s --max-time 10 "$OWL_URL/" | python3 -c "
import sys,json; d=json.load(sys.stdin)
rows=d.get('primitives') or []
for r in rows:
    if isinstance(r,dict) and r.get('kind')=='$1': print(r['collection'].replace('/v1','',1)); break"
}
# every served row of a class whose filePath names our fixture tag, as JSON lines
rows_for() { # $1 = collection
  curl -s --max-time 30 -H "Authorization: Bearer $TC" "$OWL_URL$1?limit=5000" | python3 -c "
import sys,json
for r in json.load(sys.stdin)['data']:
    if '$TAG' in r.get('filePath',''): print(json.dumps(r))"
}
crawl() { "$BIN" "$@" 2>&1; }
teardown() {
  # leave the variant as we found it: our rows only, by name, as the crawler
  for c in "$TESTS_COLL" "$FILES_COLL"; do
    [ -n "$c" ] || continue
    rows_for "$c" | python3 -c "import sys,json; [print(json.loads(l)['name']) for l in sys.stdin]" | while read -r n; do
      curl -s -o /dev/null -X DELETE -H "Authorization: Bearer $TC" --max-time 10 "$OWL_URL$c/$n" || true
    done
  done
}

@test "AC1: after a walk every case in every test file has a row, written as the crawler, inFile -> its CodeFile row" {
  run crawl; echo "$output"; [ "$status" -eq 0 ]
  has "cases posted=3 "
  rows="$(rows_for "$TESTS_COLL")"
  [ "$(printf '%s\n' "$rows" | grep -c .)" -eq 3 ]
  has "\"testName\": \"$TAG first case\"" "$rows"
  has "\"testName\": \"$TAG jest case\"" "$rows"
  # the edge: inFile names the CodeFile row the same run wrote for that file
  file_row="$(rows_for "$FILES_COLL" | grep "$BATS_F")"
  file_name="$(printf '%s' "$file_row" | python3 -c "import sys,json; print(json.load(sys.stdin)['name'])")"
  # the door SERVES an edge with its kind prefix (code-file-<name>); the value ends with our row's name
  in_file="$(printf '%s\n' "$rows" | head -1 | python3 -c 'import sys,json; print(json.load(sys.stdin)["inFile"])')"
  case "$in_file" in *"-$file_name") ;; *) echo "inFile [$in_file] does not name [$file_name]" >&2; return 1 ;; esac
  # the file lifecycle #4154 proved for the Python walker: the file rows are there too
  [ "$(rows_for "$FILES_COLL" | grep -c .)" -eq 2 ]
}

# NEGATIVE PROOF (#3734): idempotence is PROVEN — the second run writes nothing —
# and the control shows the same run DOES write when there is work.
@test "AC7a: a second run immediately after the first writes zero rows" {
  run crawl; [ "$status" -eq 0 ]; has "cases posted=3 "
  # the on-land shape: an empty delta walks nothing and writes nothing
  run crawl; echo "$output"; [ "$status" -eq 0 ]
  has "chorus-crawl: delta"
  has "cases posted=0 replaced=0 unchanged=0 deleted=0"
  has "wrote=0 failed=0"
  # the nightly shape: a FULL walk re-reads every row and still writes nothing
  rm -f "$FX/.chorus-crawl-watermark"
  run crawl; echo "$output"; [ "$status" -eq 0 ]
  has "chorus-crawl: full"
  has "cases posted=0 replaced=0 unchanged=3 deleted=0"
  has "wrote=0 failed=0"
}

@test "AC1/AC2: a case removed from its file loses its row on the next pass; a renamed case moves; a file that leaves takes its rows" {
  run crawl; [ "$status" -eq 0 ]
  # drop the second case, rename the jest case, commit — the next pass is a DELTA from the watermark
  printf '@test "%s first case" {\n  true\n}\n' "$TAG" > "$FX/$BATS_F"
  printf "it('%s renamed jest case', () => {});\n" "$TAG" > "$FX/$TS_F"
  git -C "$FX" add -A && git -C "$FX" -c user.name=t -c user.email=t@t commit -q -m edit
  run crawl; echo "$output"; [ "$status" -eq 0 ]
  has "delta"
  has "cases posted=1 replaced=0 unchanged=1 deleted=2"
  rows="$(rows_for "$TESTS_COLL")"
  lacks "second case" "$rows"
  lacks "\"testName\": \"$TAG jest case\"" "$rows"
  has "renamed jest case" "$rows"
  # the file leaves: its rows follow, one at a time
  git -C "$FX" rm -q "$TS_F" && git -C "$FX" -c user.name=t -c user.email=t@t commit -q -m rm
  run crawl; echo "$output"; [ "$status" -eq 0 ]
  has "deleted=1 "     # the file row
  has "cases posted=0 replaced=0 unchanged=0 deleted=1"   # an untouched file is not visited on a delta
  lacks "renamed jest case" "$(rows_for "$TESTS_COLL")"
  lacks "$TS_F" "$(rows_for "$FILES_COLL")"
}

# NEGATIVE PROOF (#3734, #4022): when the disk check cannot run, the crawler
# REFUSES to delete — with inputs that WOULD delete on a clean read.
@test "AC7c: a test file the crawler cannot read makes the run refuse every case delete" {
  run crawl; [ "$status" -eq 0 ]
  printf '@test "%s first case" {\n  true\n}\n' "$TAG" > "$FX/$BATS_F"     # a case vanishes...
  git -C "$FX" add -A && git -C "$FX" -c user.name=t -c user.email=t@t commit -q -m edit
  rm -f "$FX/.chorus-crawl-watermark"                                      # ...on a FULL walk (every test file is parsed)...
  chmod 000 "$FX/$TS_F"                                                    # ...and one of them cannot be read
  run crawl; echo "$output"
  chmod 644 "$FX/$TS_F"
  has "case read was PARTIAL, case deletes refused"
  has "cases posted=0 replaced=0 unchanged=1 deleted=0"
  has "watermark HELD"
  [ "$(rows_for "$TESTS_COLL" | grep -c "second case")" -eq 1 ]           # the row is still there
  # control: the same tree, readable, DOES delete — the check separates its two states
  run crawl; echo "$output"; [ "$status" -eq 0 ]
  has "cases posted=0 replaced=0 unchanged=2 deleted=1"
  [ "$(rows_for "$TESTS_COLL" | grep -c "second case")" -eq 0 ]
}

# NEGATIVE PROOF (#3734): one stale row makes the nightly reconcile RED and
# names the file — the control is the clean reconcile right before it.
@test "AC5/AC7d: the reconcile is clean after a walk and goes red naming one stale row" {
  run crawl; [ "$status" -eq 0 ]
  run crawl --reconcile; echo "$output"; [ "$status" -eq 0 ]
  has "reconcile cases: clean"
  # plant a row for a file that is not in the tree, as the crawler itself
  ghost="platform/tests/$TAG-ghost.bats"
  body="[{\"name\":\"test-$TAG-ghost-00000000\",\"filePath\":\"$ghost\",\"testName\":\"$TAG ghost case\",\"inFile\":\"$(rows_for "$FILES_COLL" | head -1 | python3 -c 'import sys,json; print(json.load(sys.stdin)["name"])')\",\"covers\":\"services\",\"pyramidLayer\":\"unit\"}]"
  run curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $TC" -H 'Content-Type: application/json' --max-time 15 -d "$body" "$OWL_URL$TESTS_COLL/batch"
  [ "$output" = "201" ]
  run crawl --reconcile; echo "$output"; [ "$status" -eq 1 ]
  has "reconcile cases: DRIFT"
  has "$ghost"
}

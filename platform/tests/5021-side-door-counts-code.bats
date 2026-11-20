#!/usr/bin/env bats
# @test-type: contract — runs the side-door counter over a fixture git tree; no store, no network.
# @domain: policies
# @card: #5021 · owner: kade
# What Jeff sees: the lint's "side door" red means code that reads a domain's store past its
# door, not a comment that names the store.
#
# AC6: domain-no-side-door skips comments (whole-line #, //, /*, *, <!--, and trailing # or //,
# never the // of a URL). NEGATIVE: a real reader still counts, including one whose line also
# carries a comment or a URL. Its baseline is re-measured without comments, 169 to 126 (shrink only).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
COUNT="$ROOT/platform/scripts/policy-count"

tree() {
  local t="$BATS_TEST_TMPDIR/tree"
  mkdir -p "$t/platform/scripts" "$t/platform/api/src" && cd "$t" && git init -q .
  cat > platform/scripts/commented <<'SH'
#!/bin/sh
# reads nothing: urn:chorus:domains:x is only named here
echo hi  # not urn:chorus:domains:x either
SH
  cat > platform/api/src/commented.ts <<'TS'
// urn:chorus:domains:x, prose only
/* urn:chorus:domains:x
 * urn:chorus:domains:x */
const ok = 1; // urn:chorus:domains:x
TS
  cat > platform/scripts/reader <<'SH'
#!/bin/sh
curl -s http://localhost:3030/pods/query --data "query=SELECT * { GRAPH <urn:chorus:domains:x> { ?s ?p ?o } }"  # reads x
SH
  printf 'const G = "urn:chorus:domains:x"; const U = "http://localhost:3030/pods";\n' > platform/api/src/reader.ts
  git add -A
}

@test "AC6: comments naming a domain's graph are not side doors; real readers are" {
  tree
  run "$COUNT" domain-no-side-door x
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(printf '%s\n' "$output" | sort | tr '\n' ' ')" = "platform/api/src/reader.ts:1 platform/scripts/reader:2 " ] || { echo "$output"; return 1; }
}

@test "AC6 NEGATIVE: uncommenting the prose makes it count" {
  tree
  sed -i '' 's|^// urn|const P = "urn|; s|, prose only|"|' platform/api/src/commented.ts
  run "$COUNT" domain-no-side-door x
  echo "$output" | grep -q '^platform/api/src/commented.ts:1$' || { echo "$output"; return 1; }
}

@test "AC6: the side-door row's baseline shrinks to the re-measured 126, never grows" {
  run python3 -c "
import json
old = [r for r in json.load(open('$ROOT/designing/domains/policies/policy-checks-5020.json')) if r['name']=='domain-no-side-door'][0]
new = [r for r in json.load(open('$ROOT/designing/domains/policies/policy-checks-5021.json')) if r['name']=='domain-no-side-door'][0]
assert int(new['baseline']) == 126 and int(new['baseline']) < int(old['baseline']), (old['baseline'], new['baseline'])
print('ok')"
  [ "$output" = ok ] || { echo "$output"; return 1; }
}

@test "AC6: land-5021-rows changes the side-door row and adds two, nothing else, and every row fits PolicyShape" {
  local T="$BATS_TEST_TMPDIR"
  python3 - "$ROOT" "$T/live.json" <<'PY'
import json, sys
rows = json.load(open(sys.argv[1] + "/designing/domains/policies/policy-checks-5020.json"))
json.dump({"policies": [{k: r[k] for k in ("name", "label", "comment", "gate", "status", "baseline", "checkKind", "checkSpec", "violationFixture")} for r in rows]}, open(sys.argv[2], "w"))
PY
  printf '#!/bin/sh\necho "$*" >> %s/runner.log\n' "$T" > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$ROOT/platform/scripts/land-5021-rows"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q 'door writes 2 add, 1 change, 0 delete' || { echo "$output"; return 1; }
  echo "$output" | grep -q '^CHANGE policies/domain-no-side-door' || { echo "$output"; return 1; }
  [ ! -e "$T/runner.log" ]
  run python3 "$ROOT/platform/scripts/land-5021-rows" --ttl "$T/rows.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\nc:principal-silas a c:Principal .\n' >> "$T/rows.ttl"
  run shacl validate --shapes "$ROOT/roles/wren/ontology/policies-4077.ttl" --data "$T/rows.ttl"
  echo "$output" | grep -q "sh:conforms *true" || { echo "$output" | grep resultMessage | head; return 1; }
}

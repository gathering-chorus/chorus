#!/usr/bin/env bats
# @test-type: contract — the folds counter against a stub of the page's endpoints, plus the fold-name parity of page, shape and counter; no store.
# @domain: principles
# @card: #5021 · owner: kade
# What Jeff sees: on /athena/domain.html?d=principles every fold is filled from real rows, or says
# why it is empty in words the Domain row carries; a fold empty without a reason is a red in the
# domain's lint.
#
# AC4: domain-folds-filled reads what the page reads (platform/tests/fixtures/5021/domain-folds-filled.sh
# stubs it) and counts each empty fold the Domain row gives no foldNote for. NEGATIVE: x's empty
# Releases with no note is red; its noted Infrastructure is not; an unreadable API is exit 2. The
# fold names in DomainShape's foldNote pattern, the page (facets.js + domain.html's Services fold)
# and the counter are one set (Silas 18:25). NEGATIVE: renaming a fold in a copy of the page fails it.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
COUNT="$ROOT/platform/scripts/policy-count"

serve() {
  local port i
  port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')"
  python3 -m http.server "$port" --bind 127.0.0.1 --directory "$1" >/dev/null 2>&1 &
  PID=$!
  for i in $(seq 1 40); do curl -sf "http://127.0.0.1:$port/owl/v1/domains/domains/${2:-y}" >/dev/null && break; sleep 0.25; done
  export CHORUS_API="http://127.0.0.1:$port"
}
teardown() { [ -n "${PID:-}" ] && { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; }; true; }

# the three lists of fold names: the shape's pattern, the page's, the counter's
fold_sets() {
  python3 - "$1" "$ROOT" <<'PY'
import re, sys
page, root = sys.argv[1], sys.argv[2]
ttl = open(f"{root}/roles/silas/ontology/chorus.ttl").read()
pattern = set(re.search(r'sh:path chorus:foldNote[^]]*sh:pattern "\^\(([^)]*)\)', ttl).group(1).split("|"))
facets = set(re.findall(r"\{ t: '([^']+)'", open(page).read())) - {"Actors", "Scenarios", "Gaps & Status"}
facets |= {"Services"}  # domain.html's own fold
count = open(f"{root}/platform/scripts/policy-count").read()
body = count[count.index("def domain_folds_filled"):count.index("DOMAIN_COUNTERS = {")]
counted = set(re.findall(r'^\s+"([A-Z][^"]+)": ', body, re.M)) - {"Description", "Vocabulary", "Design doc", "Gaps"}
print("pattern", sorted(pattern)); print("page", sorted(facets)); print("counter", sorted(counted))
sys.exit(0 if pattern == facets == counted else 1)
PY
}

@test "AC4: the shape's fold names, the page's folds and the counter's folds are one set" {
  run fold_sets "$ROOT/platform/api/public/athena/facets.js"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC4 NEGATIVE: a fold renamed on the page breaks the parity" {
  sed "s/{ t: 'Releases'/{ t: 'Shipments'/" "$ROOT/platform/api/public/athena/facets.js" > "$BATS_TEST_TMPDIR/facets.js"
  run fold_sets "$BATS_TEST_TMPDIR/facets.js"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
}

@test "AC4 NEGATIVE: x's empty Releases with no foldNote is red; its noted Infrastructure is not" {
  sh "$ROOT/platform/tests/fixtures/5021/domain-folds-filled.sh" "$BATS_TEST_TMPDIR/api"
  serve "$BATS_TEST_TMPDIR/api"
  run "$COUNT" domain-folds-filled x
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$output" = "x: Releases is empty and the Domain row gives no foldNote for it" ] || { echo "$output"; return 1; }
  run "$COUNT" domain-folds-filled y
  [ "$status" -eq 0 ] && [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "AC4 NEGATIVE: an API the counter cannot read is exit 2, never a filled page" {
  CHORUS_API=http://127.0.0.1:1 run "$COUNT" domain-folds-filled x
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "AC4: principles' notes name real folds and tell why, not 'by design'" {
  run python3 - "$ROOT" <<'PY'
import re, subprocess, sys
root = sys.argv[1]
nt = subprocess.run(["riot", "--output=nt", f"{root}/roles/wren/ontology/domains-wren-silas.ttl"], capture_output=True, text=True, check=True).stdout
notes = re.findall(r'^<https://jeffbridwell.com/chorus#principles> <https://jeffbridwell.com/chorus#foldNote> "(.*)" \.$', nt, re.M)
ttl = open(f"{root}/roles/silas/ontology/chorus.ttl").read()
pat = re.search(r'sh:path chorus:foldNote[^]]*sh:pattern "([^"]+)"', ttl).group(1)
assert notes, "no notes"
for n in notes:
    assert re.match(pat, n), n
    assert "by design" not in n.lower(), n
assert any(n.startswith("Decisions: 0 because no decision carries hasDomain yet; #4464") for n in notes), notes
print(len(notes))
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC4: the folds rule is a complete-domain policy row run per domain" {
  run python3 -c "
import json; r=[x for x in json.load(open('$ROOT/designing/domains/policies/policy-checks-5021.json')) if x['name']=='domain-folds-filled']
assert r and '{domain}' in r[0]['checkSpec'] and 'domain-folds-filled' in r[0]['checkSpec'], r; print('ok')"
  [ "$output" = ok ] || { echo "$output"; return 1; }
}

# principles' own page as the land leaves it: its Domain row built from this card's model file
# (roles/wren/ontology/domains-wren-silas.ttl, the way the door serves it: literals in data, links in
# links), the page's reads captured from prod on 11-21 (platform/tests/fixtures/5021/principles-page/,
# code/pages/endpoints/logs cut to principles' rows), and the two things only the land adds: the
# board's domain:principles cards (the After-the-land tag of 5017 and 5021) and 5017's land as a release.
# The captured reads, each named so a change to one runs this suite:
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/alerts
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/cards
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/decisions
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/dependencies
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/infra
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/pipeline
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/releases
#   platform/tests/fixtures/5021/principles-page/api/chorus/domain/principles/tests
#   platform/tests/fixtures/5021/principles-page/owl/code/endpoints
#   platform/tests/fixtures/5021/principles-page/owl/code/files
#   platform/tests/fixtures/5021/principles-page/owl/code/pages
#   platform/tests/fixtures/5021/principles-page/owl/logs/sources
principles_page() {
  local t="$BATS_TEST_TMPDIR/page"
  cp -R "$ROOT/platform/tests/fixtures/5021/principles-page" "$t"
  mkdir -p "$t/owl/v1/domains/domains"
  riot --output=nt "$ROOT/roles/wren/ontology/domains-wren-silas.ttl" | python3 -c '
import json, re, sys
data, links = {}, {}
for line in sys.stdin:
    m = re.match(r"<https://jeffbridwell.com/chorus#principles> <([^>]+)> (.+) \.$", line.strip())
    if not m:
        continue
    key = re.split(r"[#/]", m.group(1))[-1]
    o = m.group(2)
    if o.startswith("<"):
        links.setdefault(key, []).append("chorus:" + re.split(r"[#/]", o[1:-1])[-1])
    else:
        data.setdefault(key, []).append(json.loads(re.match(r"(\".*\")", o).group(1)))
json.dump({"data": data, "links": links}, sys.stdout)' > "$t/owl/v1/domains/domains/principles"
  echo "$t"
}

@test "AC4: principles' page, as the land leaves it, has every fold filled or noted (0 reds)" {
  serve "$(principles_page)" principles
  run "$COUNT" domain-folds-filled principles
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "AC4 NEGATIVE: principles' page with its Decisions note taken away is red on Decisions" {
  local t; t="$(principles_page)"
  python3 - "$t/owl/v1/domains/domains/principles" <<'PY2'
import json, sys
r = json.load(open(sys.argv[1]))
r["data"]["foldNote"] = [n for n in r["data"]["foldNote"] if not n.startswith("Decisions:")]
json.dump(r, open(sys.argv[1], "w"))
PY2
  serve "$t" principles
  run "$COUNT" domain-folds-filled principles
  [ "$output" = "principles: Decisions is empty and the Domain row gives no foldNote for it" ] || { echo "$output"; return 1; }
}

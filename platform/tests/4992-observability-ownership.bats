#!/usr/bin/env bats
# @test-type: contract — ownership plan, SHACL/policy clean, dry-run/apply/idempotency, and #3734 negative proofs
# @domain: metrics
# @card: #4992
# @owner: borg-queen
#
# #4992 AC1 — Athena Repository `shared-observability` and the five value-stream-step-proving
# observability domains (`metrics`, `monitors`, `logs`, `alerts`, `heralds`) set `ownedBy` to
# `principal-borg-queen`, with SHACL and policy validation clean.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PLAN="$ROOT/designing/domains/metrics/ownership-4992.json"
LAND="$ROOT/platform/scripts/land-4992-rows"
PV="$ROOT/platform/scripts/policy-validate"

setup() { T="$BATS_TEST_TMPDIR"; }

@test "ownership-4992.json declares shared-observability and the 5 Proving observability domains under principal-borg-queen" {
  [ -f "$PLAN" ] || { echo "missing $PLAN"; return 1; }
  run python3 - "$PLAN" <<'PY'
import json, sys
rows = json.load(open(sys.argv[1]))
want = [
    ("code", "repository", "shared-observability", "urn:chorus:domains:code"),
    ("domains", "domain", "alerts", "urn:chorus:domains:domains"),
    ("domains", "domain", "heralds", "urn:chorus:domains:domains"),
    ("domains", "domain", "logs", "urn:chorus:domains:domains"),
    ("domains", "domain", "metrics", "urn:chorus:domains:domains"),
    ("domains", "domain", "monitors", "urn:chorus:domains:domains"),
]
got = sorted((r["domain"], r["kind"], r["name"], r["graph"]) for r in rows)
assert got == sorted(want), f"unexpected targets: {got}"
for r in rows:
    assert r["ownedBy"] == "principal-borg-queen", r
    assert r["iri"].startswith("https://jeffbridwell.com/chorus#"), r
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "land-4992-rows --ttl emits valid RDF/SHACL Turtle naming principal-borg-queen on all 6 subjects, and policy-validate is clean" {
  run python3 "$LAND" --ttl "$T/ownership.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -s "$T/ownership.ttl" ] || return 1

  cat > "$T/owner-shape.ttl" <<'TTL'
@prefix c:    <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix sh:   <http://www.w3.org/ns/shacl#> .

c:OwnerPrincipalShape a sh:NodeShape ;
  sh:targetClass c:Domain , c:Repository ;
  sh:property [
    sh:path c:ownedBy ;
    sh:minCount 1 ;
    sh:maxCount 1 ;
    sh:nodeKind sh:IRI ;
    sh:class c:Principal
  ] .
TTL
  cp "$T/ownership.ttl" "$T/data-ok.ttl"
  printf 'c:principal-borg-queen a c:Principal .\n' >> "$T/data-ok.ttl"
  run shacl validate --shapes "$T/owner-shape.ttl" --data "$T/data-ok.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"sh:conforms  true"* || "$output" == *"sh:conforms true"* ]] || { echo "$output"; return 1; }

  # Negative proof (#3734): a Role IRI or two owners violates SHACL
  sed 's/c:principal-borg-queen/c:role-borg-queen/' "$T/ownership.ttl" > "$T/data-bad.ttl"
  run shacl validate --shapes "$T/owner-shape.ttl" --data "$T/data-bad.ttl"
  [[ "$output" == *"sh:conforms  false"* || "$output" == *"sh:conforms false"* ]] || { echo "$output"; return 1; }

  # Policy validation over the repo's command policy checks stays at or below baseline
  python3 - "$ROOT" "$T/policies.json" <<'PY'
import json, os, sys
root, out_path = sys.argv[1], sys.argv[2]
rows = []
for rel in ("designing/data/policy-checks-4960.json", "designing/domains/policies/policy-checks-4963.json", "designing/domains/policies/policy-checks-4964.json"):
    for r in json.load(open(os.path.join(root, rel))):
        if r.get("checkKind") == "command":
            rows.append(r)
assert len(rows) >= 20, len(rows)
json.dump({"data": rows}, open(out_path, "w"))
PY
  run sh -c "cd '$ROOT' && unset CARDS && CHORUS_HOME='$ROOT' POLICIES_JSON='$T/policies.json' '$PV'"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"0 above baseline, 0 could not run"* ]] || { echo "$output"; return 1; }
}

@test "dry-run plans 6 ownership changes when owned by kade/silas and 0 changes when already borg-queen" {
  cat > "$T/before.json" <<'JSON'
{
  "https://jeffbridwell.com/chorus#repository-shared-observability": "principal-kade",
  "https://jeffbridwell.com/chorus#alerts": "principal-silas",
  "https://jeffbridwell.com/chorus#heralds": "principal-silas",
  "https://jeffbridwell.com/chorus#logs": "principal-silas",
  "https://jeffbridwell.com/chorus#metrics": "principal-silas",
  "https://jeffbridwell.com/chorus#monitors": "principal-silas"
}
JSON
  printf '#!/bin/sh\necho "$*" >> "%s/am.log"\nexit 0\n' "$T" > "$T/am"; chmod +x "$T/am"
  LAND_LIVE_OWNERS_JSON="$T/before.json" ATHENA_MODEL_BIN="$T/am" run python3 "$LAND"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"ownership: 6 change, 0 same"* ]] || { echo "$output"; return 1; }
  [ ! -f "$T/am.log" ] || { echo "dry-run invoked athena-model"; return 1; }

  cat > "$T/after.json" <<'JSON'
{
  "https://jeffbridwell.com/chorus#repository-shared-observability": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#alerts": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#heralds": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#logs": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#metrics": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#monitors": "principal-borg-queen"
}
JSON
  LAND_LIVE_OWNERS_JSON="$T/after.json" ATHENA_MODEL_BIN="$T/am" run python3 "$LAND"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"ownership: 0 change, 6 same"* ]] || { echo "$output"; return 1; }
}

@test "apply updates all 6 rows via governed athena-model apply and verifies readback" {
  cat > "$T/state.json" <<'JSON'
{
  "https://jeffbridwell.com/chorus#repository-shared-observability": "principal-kade",
  "https://jeffbridwell.com/chorus#alerts": "principal-silas",
  "https://jeffbridwell.com/chorus#heralds": "principal-silas",
  "https://jeffbridwell.com/chorus#logs": "principal-silas",
  "https://jeffbridwell.com/chorus#metrics": "principal-silas",
  "https://jeffbridwell.com/chorus#monitors": "principal-silas"
}
JSON
  cat > "$T/am" <<'PY'
#!/usr/bin/env python3
import json, os, sys
state_file = os.environ["TEST_STATE_JSON"]
args = sys.argv[1:]
stdin = sys.stdin.read()
with open(os.environ["TEST_AM_LOG"], "a") as fh:
    fh.write(json.dumps({"args": args, "stdin": stdin}) + "\n")
assert args[0] == "apply", args
kind = args[args.index("--kind") + 1]
name = args[args.index("--name") + 1]
iri = f"https://jeffbridwell.com/chorus#{name}" if kind == "domain" else f"https://jeffbridwell.com/chorus#{kind}-{name}"
assert f'"{iri}"' not in stdin and f'"<{iri}>"' in stdin, stdin
assert '["ownedBy","principal","borg-queen"]' in stdin.replace(" ", ""), stdin
st = json.load(open(state_file))
st[iri] = "principal-borg-queen"
json.dump(st, open(state_file, "w"))
print(f"apply: 2 change(s) to {name} in one update")
PY
  chmod +x "$T/am"
  mkdir -p "$T/obs/scripts" "$T/obs/data/textfile_collector"
  git -C "$T/obs" init -q
  git -C "$T/obs" config user.name t
  git -C "$T/obs" config user.email t@t
  printf 'init\n' > "$T/obs/README.md"
  git -C "$T/obs" add README.md && git -C "$T/obs" commit -qm init

  TEST_STATE_JSON="$T/state.json" TEST_AM_LOG="$T/am.jsonl" \
    LAND_LIVE_OWNERS_JSON="$T/state.json" ATHENA_MODEL_BIN="$T/am" \
    OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" CHORUS_HOME="$ROOT" \
    run python3 "$LAND" --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(wc -l < "$T/am.jsonl" | tr -d ' ')" -eq 6 ] || { cat "$T/am.jsonl"; return 1; }
  [[ "$output" == *"ownership verified: 6/6 ownedBy principal-borg-queen"* ]] || { echo "$output"; return 1; }
}

@test "negative proof (#3734): a refused or silent-noop athena-model apply makes land-4992-rows exit 1" {
  cat > "$T/state.json" <<'JSON'
{
  "https://jeffbridwell.com/chorus#repository-shared-observability": "principal-kade",
  "https://jeffbridwell.com/chorus#alerts": "principal-silas",
  "https://jeffbridwell.com/chorus#heralds": "principal-silas",
  "https://jeffbridwell.com/chorus#logs": "principal-silas",
  "https://jeffbridwell.com/chorus#metrics": "principal-silas",
  "https://jeffbridwell.com/chorus#monitors": "principal-silas"
}
JSON
  # Case A: athena-model refuses (SHACL / authz violation)
  printf '#!/bin/sh\necho "shacl-violation: minCount" >&2\nexit 1\n' > "$T/am-fail"; chmod +x "$T/am-fail"
  mkdir -p "$T/obs/scripts" "$T/obs/data/textfile_collector"
  git -C "$T/obs" init -q && git -C "$T/obs" -c user.name=t -c user.email=t@t commit --allow-empty -qm init
  LAND_LIVE_OWNERS_JSON="$T/state.json" ATHENA_MODEL_BIN="$T/am-fail" \
    OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" CHORUS_HOME="$ROOT" \
    run python3 "$LAND" --apply
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"REFUSED"* ]] || { echo "$output"; return 1; }

  # Case B: athena-model exits 0 without actually changing ownedBy (silent no-op mutant)
  printf '#!/bin/sh\necho "ok"\nexit 0\n' > "$T/am-noop"; chmod +x "$T/am-noop"
  LAND_LIVE_OWNERS_JSON="$T/state.json" ATHENA_MODEL_BIN="$T/am-noop" \
    OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" CHORUS_HOME="$ROOT" \
    run python3 "$LAND" --apply
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"readback mismatch"* ]] || { echo "$output"; return 1; }
}

#!/usr/bin/env bats
# @test-type: contract — ownership plan, MODEL_SET TTL + retirement sync, SHACL/policy clean, single writer per row, and #3734 negative proofs
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

@test "ownership-4992.json, MODEL_SET TTL files, and model-retirements.jsonl agree on principal-borg-queen with one writer per row" {
  [ -f "$PLAN" ] || { echo "missing $PLAN"; return 1; }
  run riot --validate "$ROOT/roles/wren/ontology/domains-wren-silas.ttl" "$ROOT/roles/silas/ontology/alerts-4085.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  run python3 - "$ROOT" "$PLAN" <<'PY'
import json, os, re, sys
root, plan_path = sys.argv[1], sys.argv[2]
rows = json.load(open(plan_path))
want = [
    ("code", "repository", "shared-observability", "urn:chorus:domains:code", "athena-model"),
    ("domains", "domain", "alerts", "urn:chorus:domains:domains", "athena-deploy"),
    ("domains", "domain", "heralds", "urn:chorus:domains:domains", "athena-deploy"),
    ("domains", "domain", "logs", "urn:chorus:domains:domains", "athena-deploy"),
    ("domains", "domain", "metrics", "urn:chorus:domains:domains", "athena-deploy"),
    ("domains", "domain", "monitors", "urn:chorus:domains:domains", "athena-deploy"),
]
assert sorted((r["domain"], r["kind"], r["name"], r["graph"], r["writer"]) for r in rows) == sorted(want)
for r in rows:
    assert r["ownedBy"] == "principal-borg-queen", r

ttl_union = (
    open(os.path.join(root, "roles/wren/ontology/domains-wren-silas.ttl")).read()
    + "\n"
    + open(os.path.join(root, "roles/silas/ontology/alerts-4085.ttl")).read()
)
for d in ("alerts", "heralds", "logs", "metrics", "monitors"):
    m = re.search(rf"chorus:{d}\s+a\s+chorus:Domain\s*;(.*?)\n\s*\n", ttl_union, re.S)
    assert m, f"missing Domain block for {d}"
    block = m.group(1)
    assert "chorus:ownedBy chorus:principal-borg-queen" in block, f"{d} not ownedBy borg-queen in TTL"
    assert "chorus:ownedBy chorus:principal-silas" not in block, f"{d} still ownedBy silas in TTL"

retire_lines = [json.loads(l) for l in open(os.path.join(root, "designing/schemas/model-retirements.jsonl")) if l.strip()]
retired = {}
for r in retire_lines:
    if r.get("card") == "4992" and r.get("status") == "staged":
        assert r.get("staged_at"), f"missing staged_at on {r}"
        retired[(r.get("triple_subject"), r.get("triple_predicate"), r.get("triple_object"), r.get("graph"))] = r
for d in ("alerts", "heralds", "logs", "metrics", "monitors"):
    key = (
        f"https://jeffbridwell.com/chorus#{d}",
        "https://jeffbridwell.com/chorus#ownedBy",
        "https://jeffbridwell.com/chorus#principal-silas",
        "urn:chorus:domains:domains",
    )
    assert key in retired, f"missing staged triple retirement for {d}"
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "land-4992-rows --ttl emits valid SHACL Turtle naming principal-borg-queen on all 6 subjects, and policy-validate is clean" {
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

  # Negative proof (#3734): a Role IRI violates Principal class constraint
  sed 's/c:principal-borg-queen/c:role-borg-queen/' "$T/ownership.ttl" > "$T/data-bad.ttl"
  run shacl validate --shapes "$T/owner-shape.ttl" --data "$T/data-bad.ttl"
  [[ "$output" == *"sh:conforms  false"* || "$output" == *"sh:conforms false"* ]] || { echo "$output"; return 1; }

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

@test "dry-run plans 1 athena-model + 5 athena-deploy ownership changes before land and 0 changes after" {
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
  [[ "$output" == *"ownership: 6 change (1 athena-model, 5 athena-deploy), 0 same"* ]] || { echo "$output"; return 1; }
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
  [[ "$output" == *"ownership: 0 change (0 athena-model, 0 athena-deploy), 6 same"* ]] || { echo "$output"; return 1; }
}

@test "apply exercises the signed-in jeff token + fuseki-auth path solely for shared-observability, verifies all 6 rows, and is idempotent" {
  # Simulate post-model-before-rows state: the 5 Domain rows were updated by athena-deploy,
  # leaving repository:shared-observability for athena-model apply.
  cat > "$T/state.json" <<'JSON'
{
  "https://jeffbridwell.com/chorus#repository-shared-observability": "principal-kade",
  "https://jeffbridwell.com/chorus#alerts": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#heralds": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#logs": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#metrics": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#monitors": "principal-borg-queen"
}
JSON
  printf '#!/bin/sh\nexport ENV_SOURCED=yes\n' > "$T/chorus-env.sh"
  printf '#!/bin/sh\nexport FUSEKI_SOURCED=yes\n' > "$T/fuseki-auth.sh"
  cat > "$T/tok" <<'SH'
#!/bin/sh
echo "$1" >> "$TEST_TOK_LOG"
printf 'jwt-for-%s\n' "$1"
SH
  chmod +x "$T/tok"
  cat > "$T/am" <<'PY'
#!/usr/bin/env python3
import json, os, sys
state_file = os.environ["TEST_STATE_JSON"]
args = sys.argv[1:]
stdin = sys.stdin.read()
assert os.environ.get("CHORUS_IDENTITY_TOKEN") == "jwt-for-jeff", os.environ.get("CHORUS_IDENTITY_TOKEN")
assert os.environ.get("ENV_SOURCED") == "yes", "CHORUS_ENV_SH was not sourced"
assert os.environ.get("FUSEKI_SOURCED") == "yes", "FUSEKI_AUTH_SH was not sourced"
with open(os.environ["TEST_AM_LOG"], "a") as fh:
    fh.write(json.dumps({"args": args, "stdin": stdin}) + "\n")
assert args[0] == "apply", args
kind = args[args.index("--kind") + 1]
name = args[args.index("--name") + 1]
assert kind == "repository" and name == "shared-observability", (kind, name)
iri = f"https://jeffbridwell.com/chorus#{kind}-{name}"
assert f'"<{iri}>"' in stdin and '["ownedBy","principal","borg-queen"]' in stdin.replace(" ", ""), stdin
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

  # Even when DEPLOY_ROLE/CHORUS_ROLE name an agent, land-4992-rows strictly mints jeff
  DEPLOY_ROLE="borg-queen" CHORUS_ROLE="borg-queen" LAND_ALLOW_WERK_SOURCE=1 \
    TEST_STATE_JSON="$T/state.json" TEST_AM_LOG="$T/am.jsonl" TEST_TOK_LOG="$T/tok.log" \
    LAND_LIVE_OWNERS_JSON="$T/state.json" ATHENA_MODEL_BIN="$T/am" \
    CHORUS_ENV_SH="$T/chorus-env.sh" FUSEKI_AUTH_SH="$T/fuseki-auth.sh" IDENTITY_TOKEN_BIN="$T/tok" \
    OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" \
    run python3 "$LAND" --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(cat "$T/tok.log")" = "jeff" ] || { cat "$T/tok.log"; return 1; }
  [ "$(wc -l < "$T/am.jsonl" | tr -d ' ')" -eq 1 ] || { cat "$T/am.jsonl"; return 1; }
  [[ "$output" == *"ownership verified: 6/6 ownedBy principal-borg-queen"* ]] || { echo "$output"; return 1; }

  # Second --apply run over already-applied state is an idempotent no-op (0 new DAL writes)
  DEPLOY_ROLE="borg-queen" CHORUS_ROLE="borg-queen" LAND_ALLOW_WERK_SOURCE=1 \
    TEST_STATE_JSON="$T/state.json" TEST_AM_LOG="$T/am.jsonl" TEST_TOK_LOG="$T/tok.log" \
    LAND_LIVE_OWNERS_JSON="$T/state.json" ATHENA_MODEL_BIN="$T/am" \
    CHORUS_ENV_SH="$T/chorus-env.sh" FUSEKI_AUTH_SH="$T/fuseki-auth.sh" IDENTITY_TOKEN_BIN="$T/tok" \
    OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" \
    run python3 "$LAND" --apply
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(wc -l < "$T/am.jsonl" | tr -d ' ')" -eq 1 ] || { echo "expected 0 new DAL writes on idempotent rerun"; return 1; }
}

@test "negative proof (#3734): failed token mint, refused apply, or unapplied domain/repo owner makes land-4992-rows exit 1" {
  cat > "$T/state.json" <<'JSON'
{
  "https://jeffbridwell.com/chorus#repository-shared-observability": "principal-kade",
  "https://jeffbridwell.com/chorus#alerts": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#heralds": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#logs": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#metrics": "principal-borg-queen",
  "https://jeffbridwell.com/chorus#monitors": "principal-borg-queen"
}
JSON
  mkdir -p "$T/obs/scripts" "$T/obs/data/textfile_collector"
  git -C "$T/obs" init -q && git -C "$T/obs" -c user.name=t -c user.email=t@t commit --allow-empty -qm init
  printf '#!/bin/sh\n:\n' > "$T/fuseki-noop.sh"

  # Case A: chorus-identity-token refuses (e.g. exit 6 wrong hostAccount) -> athena-model never runs
  printf '#!/bin/sh\necho "refused — hostAccount mismatch" >&2\nexit 6\n' > "$T/tok-fail"; chmod +x "$T/tok-fail"
  printf '#!/bin/sh\necho "ran" > "%s/ran"\nexit 0\n' "$T" > "$T/am-spy"; chmod +x "$T/am-spy"
  LAND_ALLOW_WERK_SOURCE=1 LAND_LIVE_OWNERS_JSON="$T/state.json" IDENTITY_TOKEN_BIN="$T/tok-fail" \
    CHORUS_ENV_SH="$T/fuseki-noop.sh" FUSEKI_AUTH_SH="$T/fuseki-noop.sh" \
    ATHENA_MODEL_BIN="$T/am-spy" OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" \
    run python3 "$LAND" --apply
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"REFUSED token mint"* ]] || { echo "$output"; return 1; }
  [ ! -f "$T/ran" ] || { echo "athena-model ran despite failed token mint!"; return 1; }

  # Case B: athena-model refuses (SHACL / authz violation)
  printf '#!/bin/sh\nprintf "jwt-ok\\n"\n' > "$T/tok-ok"; chmod +x "$T/tok-ok"
  printf '#!/bin/sh\necho "shacl-violation: minCount" >&2\nexit 1\n' > "$T/am-fail"; chmod +x "$T/am-fail"
  LAND_ALLOW_WERK_SOURCE=1 LAND_LIVE_OWNERS_JSON="$T/state.json" IDENTITY_TOKEN_BIN="$T/tok-ok" \
    CHORUS_ENV_SH="$T/fuseki-noop.sh" FUSEKI_AUTH_SH="$T/fuseki-noop.sh" \
    ATHENA_MODEL_BIN="$T/am-fail" OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" \
    run python3 "$LAND" --apply
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"REFUSED"* ]] || { echo "$output"; return 1; }

  # Case C: athena-model exits 0 without changing ownedBy (or a Domain row still reads silas) -> readback mismatch exit 1
  printf '#!/bin/sh\necho "ok"\nexit 0\n' > "$T/am-noop"; chmod +x "$T/am-noop"
  LAND_ALLOW_WERK_SOURCE=1 LAND_LIVE_OWNERS_JSON="$T/state.json" IDENTITY_TOKEN_BIN="$T/tok-ok" \
    CHORUS_ENV_SH="$T/fuseki-noop.sh" FUSEKI_AUTH_SH="$T/fuseki-noop.sh" \
    ATHENA_MODEL_BIN="$T/am-noop" OBS_REPO="$T/obs" OBS_GROUP="$(id -gn)" \
    run python3 "$LAND" --apply
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"readback mismatch"* ]] || { echo "$output"; return 1; }
}

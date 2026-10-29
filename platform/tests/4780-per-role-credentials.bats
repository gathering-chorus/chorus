#!/usr/bin/env bats
# @test-type: contract — signal:api is local door read for live agent hostAccount check
# @domain: identity — per-role account credentials and cross-account resolution (#4780 Slice 1)

load test_helper

setup() {
  ROOT="$CHORUS_ROOT"
  SECRET_CUSTODY="$ROOT/platform/security/probes.d/secret-custody.sh"
  ENV_SETUP="$ROOT/platform/scripts/chorus-env-setup.sh"
  TS_NODE="$ROOT/directing/products/cards/node_modules/.bin/ts-node"
  if [ ! -x "$TS_NODE" ]; then
    CANONICAL_HOME="$(cd "$ROOT/../.." && pwd)/chorus"
    TS_NODE="$CANONICAL_HOME/directing/products/cards/node_modules/.bin/ts-node"
  fi
}

check_agent_host_accounts_json() {
  python3 - "$1" <<'PY'
import json, sys
rows = json.load(open(sys.argv[1])).get("data", [])
agents = [r for r in rows if r.get("principalKind") == "agent"]
if len(agents) < 5:
    print(f"expected at least 5 agent principals, got {len(agents)}")
    sys.exit(1)
bad = []
for r in sorted(agents, key=lambda x: x.get("name", "")):
    role = r.get("name", "").replace("principal-", "")
    want = f"chorus-{role}"
    got = r.get("hostAccount")
    if got != want:
        bad.append(f"principal-{role}: expected hostAccount {want!r}, got {got!r}")
if bad:
    print("\n".join(bad))
    sys.exit(1)
print("ok")
PY
}

@test "AC1: every agent principal in the live identity store binds hostAccount chorus-<role>" {
  curl -sf "http://127.0.0.1:3360/v1/identity/principals?limit=50" > "$BATS_TEST_TMPDIR/principals.json"
  run check_agent_host_accounts_json "$BATS_TEST_TMPDIR/principals.json"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC1 NEGATIVE PROOF: an agent principal bound to jeffbridwell fails the hostAccount check (#3734)" {
  cat > "$BATS_TEST_TMPDIR/good-principals.json" <<'JSON'
{"data":[
  {"name":"abby-normal","principalKind":"agent","hostAccount":"chorus-abby-normal"},
  {"name":"borg-queen","principalKind":"agent","hostAccount":"chorus-borg-queen"},
  {"name":"kade","principalKind":"agent","hostAccount":"chorus-kade"},
  {"name":"silas","principalKind":"agent","hostAccount":"chorus-silas"},
  {"name":"wren","principalKind":"agent","hostAccount":"chorus-wren"}
]}
JSON
  run check_agent_host_accounts_json "$BATS_TEST_TMPDIR/good-principals.json"
  [ "$status" -eq 0 ]

  sed 's/"hostAccount":"chorus-wren"/"hostAccount":"jeffbridwell"/' "$BATS_TEST_TMPDIR/good-principals.json" > "$BATS_TEST_TMPDIR/bad-principals.json"
  run check_agent_host_accounts_json "$BATS_TEST_TMPDIR/bad-principals.json"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"principal-wren: expected hostAccount 'chorus-wren', got 'jeffbridwell'"* ]] || return 1
}

@test "AC1 NEGATIVE PROOF: reading or minting kade's credential from another account is refused (#3734)" {
  mkdir -p "$BATS_TEST_TMPDIR/id/kade"
  printf '{"agent":"kade","hostAccount":"chorus-kade","id":"id","secret":"sec","issuer":"http://localhost:3001/","webId":"http://localhost:3001/kade/profile/card#me"}\n' > "$BATS_TEST_TMPDIR/id/kade/cred.json"
  chmod 600 "$BATS_TEST_TMPDIR/id/kade/cred.json"

  run env CHORUS_IDENTITY_DIR="$BATS_TEST_TMPDIR/id" bash "$ROOT/platform/scripts/chorus-identity-token" kade
  echo "$output"
  [ "$status" -eq 6 ]
  [[ "$output" == *"refused — 'kade' is bound to account 'chorus-kade'"* ]] || return 1
}

@test "AC1: secret-custody.sh passes on 0600 cred.json and fails on 0640/0660 (#3734)" {
  mkdir -p "$BATS_TEST_TMPDIR/home/.chorus/identity/borg-queen"
  chmod 700 "$BATS_TEST_TMPDIR/home/.chorus/identity" "$BATS_TEST_TMPDIR/home/.chorus/identity/borg-queen"
  printf '{"agent":"borg-queen"}\n' > "$BATS_TEST_TMPDIR/home/.chorus/identity/borg-queen/cred.json"
  chmod 600 "$BATS_TEST_TMPDIR/home/.chorus/identity/borg-queen/cred.json"
  HOME="$BATS_TEST_TMPDIR/home" run bash "$SECRET_CUSTODY"
  [ "$status" -eq 0 ]

  # Negative proof: 0640 cred.json is caught
  chmod 640 "$BATS_TEST_TMPDIR/home/.chorus/identity/borg-queen/cred.json"
  HOME="$BATS_TEST_TMPDIR/home" run bash "$SECRET_CUSTODY"
  [ "$status" -eq 1 ]
  [[ "$output" == *"BAD MODE:"* ]] || return 1
}

@test "AC1: place_credential in chorus-principal sets 0700 on the identity directory and 0600 on cred.json" {
  grep -q 'args(\["700", &dir\])' "$ROOT/platform/services/chorus-principal/src/main.rs"
  grep -q 'args(\["600", &path\])' "$ROOT/platform/services/chorus-principal/src/main.rs"
}

check_writer_not_jeff_for_agent() {
  local acct="$1"
  local writer="$2"
  if [[ "$acct" == chorus-* && "$writer" == "jeff" ]]; then
    echo "VIOLATION: account $acct resolved writer=jeff"
    return 1
  fi
  if [[ "$acct" == chorus-* && "$writer" != "${acct#chorus-}" ]]; then
    echo "VIOLATION: account $acct resolved writer=$writer instead of ${acct#chorus-}"
    return 1
  fi
  return 0
}

@test "AC2: cards resolveWriterRole resolves OS role, CHORUS_ROLE env fallback, and automation default" {
  run env DEPLOY_ROLE=jeff CHORUS_ROLE=wren \
    "$TS_NODE" --transpile-only --project "$ROOT/directing/products/cards/tsconfig.json" -e '
      import { resolveWriterRole } from "./directing/products/cards/src/graph-sync";
      const fromOs = resolveWriterRole("chorus-wren");
      const fromEnv = resolveWriterRole("shared-host");
      delete process.env.DEPLOY_ROLE;
      delete process.env.CHORUS_ROLE;
      const fallback = resolveWriterRole("shared-host");
      console.log(`${fromOs}:${fromEnv}:${fallback}`);
    '
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "wren:wren:automation" ]
}

@test "AC2 NEGATIVE PROOF: from chorus-silas with DEPLOY_ROLE=jeff, writer is silas and a jeff writer is caught (#3734)" {
  run env DEPLOY_ROLE=jeff CHORUS_ROLE="" \
    "$TS_NODE" --transpile-only --project "$ROOT/directing/products/cards/tsconfig.json" -e '
      import { resolveWriterRole } from "./directing/products/cards/src/graph-sync";
      console.log(resolveWriterRole("chorus-silas"));
    '
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "silas" ]

  run check_writer_not_jeff_for_agent "chorus-silas" "$output"
  [ "$status" -eq 0 ]

  # Negative proof (#3734): a naive DEPLOY_ROLE resolver returning "jeff" for chorus-silas fails the check
  run check_writer_not_jeff_for_agent "chorus-silas" "jeff"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"VIOLATION: account chorus-silas resolved writer=jeff"* ]] || return 1
}

@test "AC2: detectRole recognizes borg-queen in VALID_ROLES" {
  run env DEPLOY_ROLE=borg-queen \
    "$TS_NODE" --transpile-only --project "$ROOT/directing/products/cards/tsconfig.json" -e '
      import { detectRole } from "./directing/products/cards/src/config";
      console.log(detectRole());
    '
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "borg-queen" ]
}

@test "AC2: chorus-env-setup.sh derives CHORUS_ROLE=borg-queen and preserves an explicit caller DEPLOY_ROLE=jeff" {
  mkdir -p "$BATS_TEST_TMPDIR/roles/borg-queen"
  run env DEPLOY_ROLE=jeff bash -c "cd '$BATS_TEST_TMPDIR/roles/borg-queen' && source '$ENV_SETUP' && printf '%s:%s' \"\$CHORUS_ROLE\" \"\$DEPLOY_ROLE\""
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "borg-queen:jeff" ]
}

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
  curl -sf "http://127.0.0.1:3360/v1/identity/principals?limit=50" \
    | sed 's/"hostAccount":"chorus-wren"/"hostAccount":"jeffbridwell"/; s/"hostAccount": "chorus-wren"/"hostAccount": "jeffbridwell"/' \
    > "$BATS_TEST_TMPDIR/bad-principals.json"
  run check_agent_host_accounts_json "$BATS_TEST_TMPDIR/bad-principals.json"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"principal-wren: expected hostAccount 'chorus-wren', got 'jeffbridwell'"* ]] || return 1
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

@test "AC2: cards writerRole resolves the role account's own credential when DEPLOY_ROLE=jeff has no cred.json in HOME" {
  mkdir -p "$BATS_TEST_TMPDIR/id/wren"
  printf '{"agent":"wren"}\n' > "$BATS_TEST_TMPDIR/id/wren/cred.json"
  chmod 600 "$BATS_TEST_TMPDIR/id/wren/cred.json"

  run env CHORUS_TEST_OS_USER=chorus-wren CHORUS_IDENTITY_DIR="$BATS_TEST_TMPDIR/id" DEPLOY_ROLE=jeff CHORUS_ROLE=wren \
    "$TS_NODE" --transpile-only --project "$ROOT/directing/products/cards/tsconfig.json" -e '
      import { resolveWriterRole } from "./directing/products/cards/src/graph-sync";
      console.log(resolveWriterRole());
    '
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "wren" ]
}

@test "AC2 NEGATIVE PROOF: from chorus-silas with DEPLOY_ROLE=jeff, the row writer is silas, never jeff (#3734)" {
  mkdir -p "$BATS_TEST_TMPDIR/id/silas" "$BATS_TEST_TMPDIR/id/jeff"
  printf '{"agent":"silas"}\n' > "$BATS_TEST_TMPDIR/id/silas/cred.json"
  printf '{"agent":"jeff"}\n' > "$BATS_TEST_TMPDIR/id/jeff/cred.json"

  run env CHORUS_TEST_OS_USER=chorus-silas CHORUS_IDENTITY_DIR="$BATS_TEST_TMPDIR/id" DEPLOY_ROLE=jeff \
    "$TS_NODE" --transpile-only --project "$ROOT/directing/products/cards/tsconfig.json" -e '
      import { resolveWriterRole } from "./directing/products/cards/src/graph-sync";
      import { detectRole } from "./directing/products/cards/src/config";
      process.env.DEPLOY_ROLE = "borg-queen";
      const detected = detectRole();
      process.env.DEPLOY_ROLE = "jeff";
      console.log(`${resolveWriterRole()}:${detected}`);
    '
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "silas:borg-queen" ]
}

@test "AC2: chorus-env-setup.sh derives CHORUS_ROLE=borg-queen and preserves an explicit caller DEPLOY_ROLE=jeff" {
  mkdir -p "$BATS_TEST_TMPDIR/roles/borg-queen"
  run env DEPLOY_ROLE=jeff bash -c "cd '$BATS_TEST_TMPDIR/roles/borg-queen' && source '$ENV_SETUP' && printf '%s:%s' \"\$CHORUS_ROLE\" \"\$DEPLOY_ROLE\""
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "borg-queen:jeff" ]
}

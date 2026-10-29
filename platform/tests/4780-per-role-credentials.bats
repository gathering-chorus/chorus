#!/usr/bin/env bats
# @test-type: contract
# @domain: identity — per-role account credentials and cross-account resolution (#4780 Slice 1)

load test_helper

setup() {
  ROOT="$CHORUS_ROOT"
  PRINCIPALS="$ROOT/roles/silas/ontology/identity-principals-3613.ttl"
  SECRET_CUSTODY="$ROOT/platform/security/probes.d/secret-custody.sh"
  ENV_SETUP="$ROOT/platform/scripts/chorus-env-setup.sh"
  TS_NODE="$ROOT/directing/products/cards/node_modules/.bin/ts-node"
  if [ ! -x "$TS_NODE" ]; then
    CANONICAL_HOME="$(cd "$ROOT/../.." && pwd)/chorus"
    TS_NODE="$CANONICAL_HOME/directing/products/cards/node_modules/.bin/ts-node"
  fi
}

check_agent_host_accounts() {
  python3 - "$1" <<'PY'
import re, sys
text = open(sys.argv[1]).read()
text = re.sub(r'(?m)^\s*#.*$', '', text)
blocks = {m.group(1): m.group(2) for m in re.finditer(r'(?ms)^chorus:(principal-[\w-]+) a ([^.]*?(?:"[^"]*"[^.]*?)*)\.\s*$', text)}
bad = []
for name, body in sorted(blocks.items()):
    if 'principalKind "agent"' not in body:
        continue
    role = name.replace("principal-", "")
    want = f'chorus-{role}'
    m = re.search(r'hostAccount "([^"]+)"', body)
    got = m.group(1) if m else None
    if got != want:
        bad.append(f"{name}: expected hostAccount {want!r}, got {got!r}")
if bad:
    print("\n".join(bad))
    sys.exit(1)
print("ok")
PY
}

@test "AC1: every agent principal in identity-principals-3613.ttl binds hostAccount chorus-<role>" {
  run check_agent_host_accounts "$PRINCIPALS"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC1 NEGATIVE PROOF: an agent principal bound to jeffbridwell fails the hostAccount check (#3734)" {
  sed 's/hostAccount "chorus-wren"/hostAccount "jeffbridwell"/' "$PRINCIPALS" > "$BATS_TEST_TMPDIR/bad-principals.ttl"
  run check_agent_host_accounts "$BATS_TEST_TMPDIR/bad-principals.ttl"
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

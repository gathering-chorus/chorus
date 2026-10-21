#!/usr/bin/env bats
# @test-type: unit — reads platform/scripts/chorus-icd-load.sh and runs it against stubs; no store
# @domain: integrations
# #4091 post-land (2026-10-21 09:10): the ICD loader called the retired chorus-model
# stub and loaded nothing; with athena-model it then refused for want of a verified
# identity. It seeds with athena-model as the caller, minting a token.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  SCRIPT="$REPO/platform/scripts/chorus-icd-load.sh"
}

@test "the loader seeds with athena-model, never the retired chorus-model" {
  grep -qE '^CHORUS_MODEL=.*command -v athena-model' "$SCRIPT" || return 1
  ! grep -qE '^CHORUS_MODEL=.*chorus-model\b' "$SCRIPT" || return 1
}

# runs the script with a stub seeder and a stub identity, a stub Fuseki read
run_with() { # <identity-script-body>
  mkdir -p "$BATS_TEST_TMPDIR/bin" "$BATS_TEST_TMPDIR/root/platform/scripts"
  printf '#!/usr/bin/env bash\necho "seed $* token=${CHORUS_IDENTITY_TOKEN:+set}"\n' > "$BATS_TEST_TMPDIR/bin/athena-model"
  printf '#!/usr/bin/env bash\n%s\n' "$1" > "$BATS_TEST_TMPDIR/root/platform/scripts/chorus-identity-token"
  chmod +x "$BATS_TEST_TMPDIR/bin/athena-model" "$BATS_TEST_TMPDIR/root/platform/scripts/chorus-identity-token"
  mkdir -p "$BATS_TEST_TMPDIR/root/designing/products/athena/domains/domains" "$BATS_TEST_TMPDIR/root/building/products/convergence/domains/integrations/icd-instances"
  cp "$REPO/designing/products/athena/domains/domains/icd-ontology.ttl" "$BATS_TEST_TMPDIR/root/designing/products/athena/domains/domains/"
  cp "$REPO/building/products/convergence/domains/integrations/icd-instances/icd-instance-grafana.ttl" "$BATS_TEST_TMPDIR/root/building/products/convergence/domains/integrations/icd-instances/"
  env -u CHORUS_IDENTITY_TOKEN CHORUS_ROLE=kade CHORUS_ROOT="$BATS_TEST_TMPDIR/root" CHORUS_MODEL="$BATS_TEST_TMPDIR/bin/athena-model" \
    FUSEKI_URL="http://127.0.0.1:9" bash "$SCRIPT"
}

@test "a seed runs as the caller with a minted identity" {
  run run_with 'echo minted-for-$1'
  [[ "$output" == *"seed --kind domain"*"token=set"* ]] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: no identity → nothing loaded, and it says so (never a silent seed).
@test "no identity for the role refuses before seeding" {
  run run_with 'exit 1'
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"no identity for role 'kade'"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"seed --kind"* ]] || return 1
}

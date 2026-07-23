#!/usr/bin/env bats
# @test-type: unit — sources the helper in a subshell; no Fuseki, no live service, brings its own world.
# #3566 LOCK — the script-side write door (fuseki-auth.sh): empty unless FUSEKI_ADMIN_PASSWORD
# is set (current behavior preserved), and bash-3.2 + `set -u` safe (the empty-array crash this guards).

setup() { ROOT="$(git rev-parse --show-toplevel)"; HELPER="$ROOT/platform/scripts/fuseki-auth.sh"; }

# #3665 — since #3611 the helper's fallback reads the shared-infra credential
# file ($FUSEKI_WRITE_ENV, default ~/.gathering/data/fuseki-write.env). On a
# provisioned box that file EXISTS, so "unset env var" no longer means "no
# credential" unless the test brings its own world: point FUSEKI_WRITE_ENV at
# a path that cannot exist. (Test-contract rule: no reads of the live $HOME.)

@test "FUSEKI_AUTH is empty when FUSEKI_ADMIN_PASSWORD is unset (no auth = current behavior)" {
  run env -u FUSEKI_ADMIN_PASSWORD FUSEKI_WRITE_ENV="$BATS_TMPDIR/absent-cred.env" \
    bash -c "set -uo pipefail; source '$HELPER'; echo \${#FUSEKI_AUTH[@]}"
  [ "$status" -eq 0 ]
  [ "$output" = "0" ]
}

@test "FUSEKI_AUTH loads from the shared-infra credential file when env is unset (#3611)" {
  printf 'FUSEKI_ADMIN_USER=filewriter\nFUSEKI_ADMIN_PASSWORD=filepw\n' > "$BATS_TMPDIR/cred.env"
  run env -u FUSEKI_ADMIN_PASSWORD -u FUSEKI_ADMIN_USER FUSEKI_WRITE_ENV="$BATS_TMPDIR/cred.env" \
    bash -c "set -uo pipefail; source '$HELPER'; echo \"\${FUSEKI_AUTH[1]}\""
  [ "$status" -eq 0 ]
  [ "$output" = "filewriter:filepw" ]
}

@test "FUSEKI_AUTH carries -u admin:<pw> when the password is set" {
  run bash -c "set -uo pipefail; export FUSEKI_ADMIN_PASSWORD=sekret; source '$HELPER'; echo \"\${FUSEKI_AUTH[0]} \${FUSEKI_AUTH[1]}\""
  [ "$status" -eq 0 ]
  [ "$output" = "-u admin:sekret" ]
}

@test "honors FUSEKI_ADMIN_USER override" {
  run bash -c "set -uo pipefail; export FUSEKI_ADMIN_PASSWORD=p FUSEKI_ADMIN_USER=chorus-writer; source '$HELPER'; echo \"\${FUSEKI_AUTH[1]}\""
  [ "$status" -eq 0 ]
  [ "$output" = "chorus-writer:p" ]
}

@test "empty FUSEKI_AUTH expands safely under set -u on bash 3.2 (the regression guard)" {
  cat > "$BATS_TMPDIR/probe.sh" <<EOF
set -uo pipefail
source "$HELPER"
args=(curl "\${FUSEKI_AUTH[@]+"\${FUSEKI_AUTH[@]}"}" END)
echo "\${#args[@]}"
EOF
  run env -u FUSEKI_ADMIN_PASSWORD FUSEKI_WRITE_ENV="$BATS_TMPDIR/absent-cred.env" bash "$BATS_TMPDIR/probe.sh"
  [ "$status" -eq 0 ]
  [ "$output" = "2" ]   # curl + END only — no empty "" arg injected when unauthenticated
}

#!/usr/bin/env bats
# @test-type: e2e — after the land: the installed chorus-principal status against the live store
# @domain: identity
# @card: #4997
#
# AC3 (after test): every live role's status matches its Session row. A role whose status
# says "logged in" must have its Session row open in the store right now (the open-sessions
# listing, endedAt absent). Read-only: status and one GET.

setup() {
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
  BIN="${CHORUS_PRINCIPAL_BIN:-$(command -v chorus-principal || echo "$HOME/.chorus/bin/chorus-principal")}"
}

# $1 status output, $2 open-sessions listing json: 0 when every "logged in" role has an open row
verdict() {
  python3 - "$1" "$2" <<'PY'
import json, re, sys
status, listing = sys.argv[1], json.loads(sys.argv[2])
open_owners = {r.get("ownedBy", "").rsplit("#", 1)[-1].removeprefix("principal-") for r in listing.get("data", [])}
bad = []
for line in status.splitlines():
    m = re.match(r"^(\S+)\s+running\s+logged in\b", line)
    if m and m.group(1) not in open_owners: bad.append(m.group(1))
print("logged in without an open Session row: " + (", ".join(bad) or "none"))
sys.exit(1 if bad else 0)
PY
}

@test "every role the status calls logged in has its Session row open in the store" {
  case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) skip "after-land check: a card's werk has not landed (#4997 AC3)";; esac
  out=$("$BIN" status 2>&1)
  listing=$(curl -sf --max-time 10 "$DOOR/v1/identity/sessions?endedAt.absent=true&limit=5000")
  run verdict "$out" "$listing"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a role called logged in with no open row fails" {
  run verdict "kade   running  logged in      answering" '{"data":[{"ownedBy":"principal-wren"}]}'
  [ "$status" -eq 1 ]
  run verdict "kade   running  logged in      answering" '{"data":[{"ownedBy":"https://jeffbridwell.com/chorus#principal-kade"}]}'
  [ "$status" -eq 0 ]
}

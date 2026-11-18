#!/usr/bin/env bats
# @test-type: e2e — after the land: the installed chorus-principal status against the live store
# @domain: identity
# @card: #4997
#
# AC3 (after test): every live role's status matches its Session row, both ways: "logged in"
# needs the row open in the store (open-sessions listing, endedAt absent), and an open row is
# never "NOT logged in". Read-only: status and one GET.

setup() {
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
  BIN="${CHORUS_PRINCIPAL_BIN:-$(command -v chorus-principal || echo "$HOME/.chorus/bin/chorus-principal")}"
}

# $1 status output, $2 open-sessions listing json: 0 when status and rows agree both ways —
# "logged in" needs an open row, and an open row is never called "NOT logged in"
# ("unverified" is honest either way: the login was out of this account's reach).
verdict() {
  python3 - "$1" "$2" <<'PY'
import json, re, sys
status, listing = sys.argv[1], json.loads(sys.argv[2])
open_owners = {r.get("ownedBy", "").rsplit("#", 1)[-1].removeprefix("principal-") for r in listing.get("data", [])}
bad = []
for line in status.splitlines():
    m = re.match(r"^(\S+)\s+running\s+(logged in|NOT logged in)\b", line)
    if not m: continue
    role, word = m.groups()
    if word == "logged in" and role not in open_owners: bad.append(role + ": logged in, no open row")
    if word == "NOT logged in" and role in open_owners: bad.append(role + ": NOT logged in, row open")
print("status and Session rows disagree: " + ("; ".join(bad) or "none"))
sys.exit(1 if bad else 0)
PY
}

@test "every live role's status matches its Session row, both directions" {
  case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) skip "after-land check: a card's werk has not landed (#4997 AC3)";; esac
  out=$("$BIN" status 2>&1)
  listing=$(curl -sf --max-time 10 "$DOOR/v1/identity/sessions?endedAt.absent=true&limit=5000")
  run verdict "$out" "$listing"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a mismatch either way fails; agreement and unverified pass" {
  run verdict "kade   running  logged in      answering" '{"data":[{"ownedBy":"principal-wren"}]}'
  [ "$status" -eq 1 ]
  run verdict "kade   running  NOT logged in  answering" '{"data":[{"ownedBy":"principal-kade"}]}'
  [ "$status" -eq 1 ]
  run verdict "kade   running  logged in      answering" '{"data":[{"ownedBy":"https://jeffbridwell.com/chorus#principal-kade"}]}'
  [ "$status" -eq 0 ]
  run verdict "kade   running  unverified     answering" '{"data":[{"ownedBy":"principal-kade"}]}'
  [ "$status" -eq 0 ]
}

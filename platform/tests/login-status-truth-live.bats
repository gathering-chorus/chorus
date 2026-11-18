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

# $1 status output, $2 open-sessions listing json: 0 when status and the role's OWN Session row
# agree both ways. The role's row is the one status names on its "store: session <name>" line
# (its saved row; a superseded row of the same role may still be open and does not count).
# "logged in" needs that row open; a role whose row is open is never "NOT logged in".
# "unverified" is honest either way: the login was out of this account's reach.
verdict() {
  python3 - "$1" "$2" <<'PY'
import json, re, sys
status, listing = sys.argv[1], json.loads(sys.argv[2])
open_names = {r.get("name") for r in listing.get("data", [])}
rows, words, role = {}, {}, None
for line in status.splitlines():
    m = re.match(r"^(\S+)\s+running\s+(logged in|NOT logged in)\b", line)
    if m: role = m.group(1); words[role] = m.group(2); continue
    if re.match(r"^\S", line): role = None; continue
    s = re.match(r"^\s+store: session (\S+) ", line)
    if s and role: rows[role] = s.group(1)
bad = []
for role, word in words.items():
    is_open = rows.get(role) in open_names
    if word == "logged in" and not is_open: bad.append(role + ": logged in, its row not open")
    if word == "NOT logged in" and is_open: bad.append(role + ": NOT logged in, its row open")
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

@test "NEGATIVE PROOF: a mismatch either way fails; agreement, unverified and a superseded open row pass" {
  st_in=$'kade   running  logged in      answering\n       store: session kade-2 open, acts as role-kade'
  st_out=$'kade   running  NOT logged in  answering\n       store: session kade-2 open, acts as role-kade'
  run verdict "$st_in" '{"data":[{"name":"wren-1"}]}'
  [ "$status" -eq 1 ]
  run verdict "$st_out" '{"data":[{"name":"kade-2"}]}'
  [ "$status" -eq 1 ]
  run verdict "$st_in" '{"data":[{"name":"kade-2"}]}'
  [ "$status" -eq 0 ]
  # an older kade row still open, its own row closed: NOT logged in is right
  run verdict $'kade   running  NOT logged in  answering' '{"data":[{"name":"kade-1","ownedBy":"principal-kade"}]}'
  [ "$status" -eq 0 ]
  run verdict "kade   running  unverified     answering" '{"data":[{"name":"kade-2"}]}'
  [ "$status" -eq 0 ]
}

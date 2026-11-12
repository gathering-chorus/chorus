#!/usr/bin/env bats
# @test-type: smoke — live: Borg Queen's bypasses 1 and 4 re-run after the land and the lander install
# @domain: security
# @needs: machine — prod after the land, the lander installed, Jeff's phone re-registered on it
# @card: #4710 · owner: wren
# What Jeff sees: the two holes Borg Queen found on 10-23 that were still open are closed.
#   1  a role reaching an admin GitHub credential  → see 4710-roles-hold-no-admin-live.bats (run as a role)
#   4  a jeffbridwell process swapping passkey.json → this file: the key Jeff's go is checked
#      against is the lander's, in a dir and file only the lander can write
load test_helper

lander_public() {
  local h; h="$(dscl . -read /Users/chorus-lander NFSHomeDirectory 2>/dev/null | awk '{print $2}')"
  [ -n "$h" ] && echo "$h/public"
}

# prints why the key in $1 could be swapped by an account other than uid $2, if it could
swap_problems() {
  local d="$1" u="$2"
  [ -f "$d/passkey.json" ] || { echo "no key at $d/passkey.json"; return 0; }
  for p in "$d" "$d/passkey.json"; do
    [ "$(stat -f %u "$p")" = "$u" ] || echo "$p is owned by uid $(stat -f %u "$p"), not the lander"
    (( (8#$(stat -f %Lp "$p") & 8#022) == 0 )) || echo "$p is writable by others"
  done
  return 0
}

@test "AC7 bypass 4: Jeff's key lives in the lander's public dir, and no other account can write or swap it" {
  d="$(lander_public)" || { echo "unmeasured: no chorus-lander account"; return 1; }
  out="$(swap_problems "$d" "$(id -u chorus-lander)")"
  [ -z "$out" ] || { echo "$out"; return 1; }
  ! ( : > "$d/passkey.json.swap" ) 2>/dev/null || { rm -f "$d/passkey.json.swap"; echo "this account could write into $d"; return 1; }
}

@test "AC7 bypass 4: the land refuses when the lander holds no key (fail closed)" {
  run env -i HOME="$BATS_TEST_TMPDIR" PATH=/usr/bin:/bin "$(dscl . -read /Users/chorus-lander NFSHomeDirectory | awk '{print $2}')/bin/chorus-go" check 1 0000000000000000000000000000000000000000
  [ "$status" -ne 0 ] || { echo "a check with no matching go passed: $output"; return 1; }
}

@test "AC7 negative proof: a key in a dir another account can write (the 10-23 state) is caught" {
  d="$BATS_TEST_TMPDIR/pub"; mkdir -p "$d"; : > "$d/passkey.json"; chmod 775 "$d"
  out="$(swap_problems "$d" "$(id -u)")"
  [[ "$out" == *"writable by others"* ]] || { echo "$out"; return 1; }
  out="$(swap_problems "$d" "$(( $(id -u) + 1 ))")"
  [[ "$out" == *"not the lander"* ]] || { echo "$out"; return 1; }
}

# AC7 bypass 1 (Silas 18:33 item 11): a role reaching an admin GitHub credential — re-run here
@test "AC7 bypass 1: a role holds no GitHub credential and GitHub refuses its three moves" {
  case "$(id -un)" in chorus-*) ;; *) echo "unmeasured: bypass 1 is run as a role account, not $(id -un)"; return 1;; esac
  run bats "$CHORUS_ROOT/platform/tests/4710-roles-hold-no-admin-live.bats"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

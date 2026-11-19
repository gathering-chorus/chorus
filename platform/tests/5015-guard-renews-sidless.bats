#!/usr/bin/env bats
# @test-type: unit:security — drives the guard's real session functions and person_session.open_session with a stub door
# @domain: security — Jeff stays signed in through an idle hour (#5015 AC2)
# @card: #5015
# @owner: wren
#
# Jeff, 11-18: signed out whenever his phone sits about an hour. A browser sign-in the
# door refused (#4517: "login has no id") got a cookie with no sid, which the guard never
# renews (#4500 keeps sid-less cookies unrenewed: they cannot be signed out). With #5015
# AC1 the door records the login, so the cookie carries the row's sid and renews.
#
# Covers: platform/scripts/chorus-share-guard.py
# Covers: platform/scripts/lib/person_session.py
# Covers: platform/tests/fixtures/signed-in-renews-5015.py

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  FIX="$ROOT/platform/tests/fixtures/signed-in-renews-5015.py"
}

@test "a recorded sign-in renews after an idle hour; a refused one carries no sid and does not" {
  run python3 "$FIX"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$(grep -c '^PASS ' <<<"$output")" -eq 4 ]
}

@test "NEGATIVE PROOF: a guard that never renews a sid'd cookie fails the fixture" {
  sed 's/    if not sid or int(claims.get("exp", 0)) - now > ttl - renew_every:/    if True:/' \
    "$ROOT/platform/scripts/chorus-share-guard.py" > "$BATS_TEST_TMPDIR/guard.py"
  if cmp -s "$ROOT/platform/scripts/chorus-share-guard.py" "$BATS_TEST_TMPDIR/guard.py"; then echo "mutation did not apply"; return 1; fi
  GUARD_5015="$BATS_TEST_TMPDIR/guard.py" run python3 "$FIX"
  echo "$output"
  [ "$status" -eq 1 ]
  grep -q '^FAIL door 201: an idle hour later the sign-in is ok and renewed' <<<"$output"
}

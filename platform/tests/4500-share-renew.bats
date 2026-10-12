#!/usr/bin/env bats
# @test-type: unit:security — drives the guard's real session functions with stub I/O; no network, no store
# @domain: security — the shared sign-in door keeps him signed in while he uses it (#4500)
# @card: #4500
# @owner: wren
#
# Jeff, 2026-10-12: "if i go to sleep and then return when i wake i typically
# must login again". The door's sign-in lasted 12 hours from sign-in, however
# much he used it. Now use renews it, up to 30 days after sign-in; signed out
# or revoked fails on the next request (Silas's two asks, 08:08).
#
# Covers: platform/scripts/chorus-share-guard.py
# Covers: platform/scripts/lib/person_session.py
# Covers: platform/tests/fixtures/share-renew-4500.py

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  FIX="$ROOT/platform/tests/fixtures/share-renew-4500.py"
}

@test "use renews his sign-in, 30 days is the end, signed out or revoked never renews" {
  run python3 "$FIX"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" != *FAIL* ]] || return 1
}

@test "NEGATIVE PROOF: with the 30-day cap removed from the guard, the fixture fails" {
  work="$BATS_TEST_TMPDIR/scripts"
  mkdir -p "$work" && cp -R "$ROOT/platform/scripts/lib" "$work/lib"
  sed '/int(claims\["iat"\]) + max_life <= now/{N;d;}' "$ROOT/platform/scripts/chorus-share-guard.py" > "$work/chorus-share-guard.py"
  # the edit must have happened, or this proof proves nothing
  run grep -c 'int(claims\["iat"\]) + max_life <= now' "$work/chorus-share-guard.py"
  [ "$output" = "0" ]
  sed "s#os.path.join(HERE, \"..\", \"..\", \"scripts\", \"chorus-share-guard.py\")#\"$work/chorus-share-guard.py\"#" "$FIX" > "$BATS_TEST_TMPDIR/fix.py"
  sed -i '' "s#os.path.join(HERE, \"..\", \"..\", \"scripts\", \"lib\")#\"$work/lib\"#" "$BATS_TEST_TMPDIR/fix.py"
  run python3 "$BATS_TEST_TMPDIR/fix.py"
  [ "$status" -ne 0 ]
  [[ "$output" == *"FAIL NEGATIVE: a cookie signed with a long exp"* ]] || return 1
}

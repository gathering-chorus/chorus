#!/usr/bin/env bats
# @test-type: unit:security — loads the door's real code with its state in a temp dir; no network, no store, no running guard
# @domain: identity
# @card: #4966
#
# Measured 2026-11-14 21:12:49: the guard logged jeff, with a session 59 minutes old, at
# /_auth/?next=<the Clearing>, and showed him the sign-in page; he signed in again. A caller
# whose session is good is sent on to where they were going.
#
# Covers: platform/scripts/chorus-share-guard.py
# Covers: platform/tests/fixtures/signed-in-landing-4966.py

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  FIX="$ROOT/platform/tests/fixtures/signed-in-landing-4966.py"
  GUARD="$ROOT/platform/scripts/chorus-share-guard.py"
}

@test "a good session at /_auth/ is sent on; every other state gets the sign-in page; never another site" {
  run python3 "$FIX"
  echo "$output"
  [ "$status" -eq 0 ]
  if printf '%s\n' "$output" | grep -q '^FAIL'; then return 1; fi
}

@test "the /_auth/ route asks it, with the session's own state, so the check covers what a browser hits" {
  grep -q 'target = signed_in_landing(state, nxt)' "$GUARD"
}

@test "NEGATIVE PROOF: a door that sends everyone on, signed in or not, fails" {
  work="$BATS_TEST_TMPDIR/scripts"
  mkdir -p "$work" && cp -R "$ROOT/platform/scripts/lib" "$work/lib"
  # drop the two lines that stop a caller without a good session
  sed '/    if state != "ok":/{N;d;}' "$GUARD" > "$work/chorus-share-guard.py"
  # the edit must have happened, or this proof proves nothing
  if cmp -s "$GUARD" "$work/chorus-share-guard.py"; then echo "mutation did not apply"; return 1; fi
  run python3 "$FIX" "$work/chorus-share-guard.py"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -q '^FAIL anonymous gets the sign-in page, not a redirect'
}

@test "NEGATIVE PROOF: a door that still shows the sign-in page to a good session fails" {
  work="$BATS_TEST_TMPDIR/scripts"
  mkdir -p "$work" && cp -R "$ROOT/platform/scripts/lib" "$work/lib"
  sed 's/    target = safe_return(next_raw)/    return None/' "$GUARD" > "$work/chorus-share-guard.py"
  if cmp -s "$GUARD" "$work/chorus-share-guard.py"; then echo "mutation did not apply"; return 1; fi
  run python3 "$FIX" "$work/chorus-share-guard.py"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -q '^FAIL a good session is sent on to where it was going'
}

@test "NEGATIVE PROOF: a door that sends a good session on to its own sign-out fails" {
  work="$BATS_TEST_TMPDIR/scripts"
  mkdir -p "$work" && cp -R "$ROOT/platform/scripts/lib" "$work/lib"
  sed '/    if AUTH_PREFIX in urllib.parse.urlparse(target).path + "\/":/{N;d;}' "$GUARD" > "$work/chorus-share-guard.py"
  if cmp -s "$GUARD" "$work/chorus-share-guard.py"; then echo "mutation did not apply"; return 1; fi
  run python3 "$FIX" "$work/chorus-share-guard.py"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -q "^FAIL a good session with next=/_auth/logout lands on the default"
}

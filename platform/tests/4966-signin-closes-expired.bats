#!/usr/bin/env bats
# @test-type: unit:security — drives the door's session library with a stub door; no network, no store, no running guard
# @domain: identity
# @card: #4966
#
# AC6 (Jeff 2026-11-15 13:27 "how do we clean it up?", 13:34 "yes"): when Jeff signs in, the door
# uses that new sign-in, as him, to close his earlier browser sessions whose expiresAt has passed.
# No role writes his rows. Measured at the door 21:16: 123 open-but-expired, 1 live.
#
# Covers: platform/scripts/lib/person_session.py
# Covers: platform/scripts/chorus-share-guard.py
# Covers: platform/tests/fixtures/expired-sessions-4966.py

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
  FIX="$ROOT/platform/tests/fixtures/expired-sessions-4966.py"
  LIB="$ROOT/platform/scripts/lib"
  GUARD="$ROOT/platform/scripts/chorus-share-guard.py"
}

@test "his expired open browser sessions are closed with his new sign-in; nothing else is touched" {
  run python3 "$FIX"
  echo "$output"
  [ "$status" -eq 0 ]
  if printf '%s\n' "$output" | grep -q '^FAIL'; then return 1; fi
}

@test "the door does it at his sign-in, after the new session is written, with that sign-in's token" {
  grep -q 'person_session.close_expired_sessions(rec\["principal"\], rec\["id_token"\], now, live, _http, ATHENA_URL, _log)' "$GUARD"
  # after the new row exists: the sign-in calls it below the line that stores the new sid
  new_line=$(grep -n 'recs\[sid\] = rec' "$GUARD" | head -1 | cut -d: -f1)
  call_line=$(grep -n '^            close_expired_person_sessions(rec, now, live)' "$GUARD" | head -1 | cut -d: -f1)
  [ -n "$call_line" ] && [ "$call_line" -gt "$new_line" ]
}

@test "a test run never reaches the real door: in test mode the close makes no call" {
  grep -q 'if OFFLINE or not rec.get("principal") or not rec.get("id_token"):' "$GUARD"
}

@test "NEGATIVE PROOF: a library that closes sessions that have not expired fails" {
  work="$BATS_TEST_TMPDIR/lib"
  mkdir -p "$work" && cp "$LIB"/*.py "$work/"
  sed 's/        if exp > iso(now):/        if False:/' "$LIB/person_session.py" > "$work/person_session.py"
  if cmp -s "$LIB/person_session.py" "$work/person_session.py"; then echo "mutation did not apply"; return 1; fi
  run python3 "$FIX" "$work"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -q '^FAIL NEGATIVE: an unexpired earlier session of his stays open'
}

@test "NEGATIVE PROOF: a library that closes agent sessions fails" {
  work="$BATS_TEST_TMPDIR/lib"
  mkdir -p "$work" && cp "$LIB"/*.py "$work/"
  sed '/        if r.get("channel") != "browser":/{N;d;}' "$LIB/person_session.py" > "$work/person_session.py"
  if cmp -s "$LIB/person_session.py" "$work/person_session.py"; then echo "mutation did not apply"; return 1; fi
  run python3 "$FIX" "$work"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -q '^FAIL NEGATIVE: an agent (pane) session is never closed by expiry'
}

@test "NEGATIVE PROOF: a library that closes another person's sessions fails" {
  work="$BATS_TEST_TMPDIR/lib"
  mkdir -p "$work" && cp "$LIB"/*.py "$work/"
  sed '/        if bare(r.get("ownedBy")) != principal:/{N;d;}' "$LIB/person_session.py" > "$work/person_session.py"
  if cmp -s "$LIB/person_session.py" "$work/person_session.py"; then echo "mutation did not apply"; return 1; fi
  run python3 "$FIX" "$work"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -q "^FAIL NEGATIVE: another person's expired session is not his to close"
}

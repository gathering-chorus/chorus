#!/usr/bin/env bats
# @test-type: unit — drives the session helpers with stub I/O; no network, no store
# @domain: security — the shared sign-in door writes his Session (#4412)
#
# Jeff signs in at the shared door, and the Clearing accepts that cookie, so the
# Clearing's own sign-in (which wrote his Session) never ran: 0 rows for
# principal-jeff on 2026-10-10. The door that signs him in now writes the row,
# as him, and closes it at sign-out.
#
# Covers: platform/scripts/lib/person_session.py
# Covers: platform/tests/fixtures/person-session-4412.py
# Covers: platform/scripts/chorus-share-guard.py
# Covers: designing/data/service-instances.ttl
# Covers: platform/services/athena-model/src/lib.rs
# Covers: platform/services/athena-make/src/lib.rs

setup() {
  ROOT="${BATS_TEST_DIRNAME}/../.."
}

@test "the shared door writes, binds and closes his Session, and refuses what it must" {
  run python3 "$ROOT/platform/tests/fixtures/person-session-4412.py"
  echo "$output"
  [ "$status" -eq 0 ]
  if [[ "$output" == *FAIL* ]]; then return 1; fi
}

@test "the sign-in row carries the hostAccount SessionShape requires, or is not written" {
  run python3 "$ROOT/platform/tests/fixtures/person-session-4412.py"
  grep -qF "PASS it carries the hostAccount the shape requires" <<<"$output" || return 1
  grep -qF "PASS NEGATIVE: no hostAccount -> no row, said in the log" <<<"$output" || return 1
}

@test "the guard is a public client named by a document it serves, with no registration" {
  G="$ROOT/platform/scripts/chorus-share-guard.py"
  grep -q 'AUTH_PREFIX + "clientid.jsonld"' "$G"
  grep -q 'open_person_session(webid, tok)' "$G"
  grep -q 'close_person_session(self._session_claims())' "$G"
  if grep -q 'def register_client' "$G"; then echo "dynamic registration still present"; return 1; fi
  if grep -q 'client_secret' "$G"; then echo "a client secret is still sent"; return 1; fi
}

@test "the model names both sign-in surfaces the door trusts" {
  F="$ROOT/designing/data/service-instances.ttl"
  grep -q 'oidcClientId "https://clearing.lightlifeurbangardens.com/clientid.jsonld"' "$F"
  grep -q 'oidcClientId "https://chorus.lightlifeurbangardens.com/_auth/clientid.jsonld"' "$F"
}

# Every token verifier the door's write path builds must know the model's
# sign-in clients. On 10-11 athena-make's did and athena-model's (the writer the
# door hands a row to) did not, so the guard's sign-in was refused at the write.
unwired() { # src files... → names each file that builds a verifier without the clients
  for f in "$@"; do
    n=$(grep -c 'OidcVerifier::new(' "$f"); w=$(grep -c 'with_person_clients(' "$f")
    [ "$n" -eq 0 ] || [ "$w" -ge "$n" ] || echo "$f"
  done
}

@test "every verifier on the write path reads the model's sign-in clients" {
  srcs=("$ROOT/platform/services/athena-make/src/lib.rs" "$ROOT/platform/services/athena-model/src/lib.rs")
  for f in "${srcs[@]}"; do grep -q 'OidcVerifier::new(' "$f" || { echo "no verifier in $f (guard target gone)"; return 1; }; done
  out="$(unwired "${srcs[@]}")"
  [ -z "$out" ] || { echo "builds a verifier without with_person_clients: $out"; return 1; }
}

@test "NEGATIVE PROOF: a verifier built without the sign-in clients is named" {
  cp "$ROOT/platform/services/athena-model/src/lib.rs" "$BATS_TEST_TMPDIR/lib.rs"
  sed -i '' 's/\.with_person_clients(/.dropped(/' "$BATS_TEST_TMPDIR/lib.rs"
  out="$(unwired "$BATS_TEST_TMPDIR/lib.rs")"
  [ "$out" = "$BATS_TEST_TMPDIR/lib.rs" ] || return 1
}

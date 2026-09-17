#!/usr/bin/env bats
# @test-type: contract
# #3830 — chorus-principal. Every test here is a REFUSAL, because the card is
# about the states a half-provisioned user can be left in, and each refusal is
# one of them made unreachable.
#
# Measured baseline this card exists for (2026-09-15 12:47): 13 principals,
# three of them — crawler-index, reindex-worker, embed-worker — naming WebIDs
# whose profile card answers 401. Principal rows typed by hand into a TTL while
# seed-css.sh's AGENTS list decided which pods actually got made. Two lists,
# nothing reconciling them.

BIN="${BATS_TEST_DIRNAME}/../services/chorus-principal/target/release/chorus-principal"

STUB="${BATS_TEST_DIRNAME}/3830-stub.py"

setup() {
  [ -x "$BIN" ] || skip "chorus-principal not built"
  T="$(mktemp -d)"
  export FUSEKI_URL="http://127.0.0.1:59998"        # nothing listens
  export CSS_URL="http://127.0.0.1:59999"           # nothing listens — the REGISTER is dead in every test
  export ATHENA_MAKE_URL="http://127.0.0.1:59997"   # nothing listens
  unset CHORUS_IDENTITY_TOKEN CHORUS_ROLE DEPLOY_ROLE
}
teardown() { rm -rf "$T"; [ -n "${STUB_PID:-}" ] && kill "$STUB_PID" 2>/dev/null; true; }

# One stub for the store, the discovery document and the profile cards.
# $1 = card status for ghost (row without a pod), $2 = for whole.
world() {
  PORT=$((20000 + RANDOM % 20000))
  python3 "$STUB" "$PORT" "$1" "$2" & STUB_PID=$!
  for _ in $(seq 1 40); do curl -s -o /dev/null "http://127.0.0.1:$PORT/" && break; sleep 0.1; done
  export FUSEKI_URL="http://127.0.0.1:$PORT" ATHENA_MAKE_URL="http://127.0.0.1:$PORT"
}

@test "usage is exit 2, not a silent success" {
  run "$BIN"
  [ "$status" -eq 2 ]
  [[ "$output" == *"create"* ]]
}

@test "NEGATIVE PROOF — an empty read is REFUSED, never reported as a clean census" {
  # The census reads the register through SPARQL. With the store unreachable it
  # reads zero principals. Zero principals and a healthy system are identical
  # from the inside, so the only honest answer is a refusal.
  #
  # This is not hypothetical: the first version of the JSON reader matched the
  # needle `"value":"` while Jena pretty-prints `"value" : "`, and it read 0 of
  # 13 bindings from a perfectly good response. This refusal is what surfaced it.
  run "$BIN" census
  [ "$status" -eq 2 ]
  [[ "$output" == *"refusing to report a clean census over an empty read"* ]]
}

@test "NEGATIVE PROOF — a role store that cannot be read is UNMEASURED, not no-role" {
  # A failed read is not a FALSE. Read the other way, an unreachable roles
  # domain refuses every provision and calls it a policy decision.
  run "$BIN" create somebody-new --kind agent
  [ "$status" -eq 2 ]
  [[ "$output" == *"UNMEASURED, not no-role"* ]]
  [[ "$output" == *"nothing was written"* ]]
}

@test "NEGATIVE PROOF — the unreadable store is a REFUSAL, not a yes (v1 returned true here)" {
  # The first version logged a WARN and returned true: every Fuseki outage
  # became a minted credential. Under the same dead store, nothing past the
  # gate may run — the register refusal text must NOT appear.
  run "$BIN" create somebody-new --kind agent
  [[ "$output" != *"could not reach the CSS accounts API"* ]]
}

@test "a human is not gated on a role — they act for themself" {
  # Same dead roles store, --kind human: the role gate does not fire. The run
  # gets as far as the identity check, which refuses for its own reason.
  run "$BIN" create somebody-new --kind human --name "Some Body" --email somebody@example.org
  [ "$status" -eq 2 ]
  [[ "$output" != *"UNMEASURED"* ]]
  [[ "$output" == *"no verified identity"* ]]
}

@test "NEGATIVE PROOF — a human with no name or no email is REFUSED at the door" {
  run "$BIN" create somebody-new --kind human --email somebody@example.org
  [ "$status" -eq 2 ]
  [[ "$output" == *"needs --name"* ]]
  run "$BIN" create somebody-new --kind human --name "Some Body"
  [ "$status" -eq 2 ]
  [[ "$output" == *"needs --name"* ]]
}

@test "a human gets their OWN account — the register unreachable refuses before anything, account included" {
  world 401 200
  export CHORUS_IDENTITY_TOKEN="test-token"
  run "$BIN" create somebody-new --kind human --name "Some Body" --email somebody@example.org
  [ "$status" -eq 2 ]
  [[ "$output" == *"could not reach the CSS accounts API"* ]]
  [[ "$output" == *"nothing was written"* ]]
  [ ! -e "$HOME/.chorus/identity/somebody-new/initial-password" ]
}

@test "NEGATIVE PROOF — a user with no kind is REFUSED at the door, not minted" {
  run "$BIN" create somebody-new
  [ "$status" -eq 2 ]
  [[ "$output" == *"has no kind"* ]]
  [[ "$output" == *"nothing was written"* ]]
}

@test "NEGATIVE PROOF — a principal ROW without a pod is HALF-PROVISIONED, never ALREADY EXISTS" {
  # crawler-index today: a row names a webId whose profile card answers 401.
  # The first version answered "already exists" and handed back that webId —
  # a pointer to nothing, stamped as a no-op. The register decides existence.
  world 401 200
  export CHORUS_IDENTITY_TOKEN="test-token"
  run "$BIN" create ghost --kind agent
  [[ "$output" != *"already exists"* ]]
  [[ "$output" == *"HALF-PROVISIONED"* ]]
  # ...and it went on to the register, which is dead here, so it refused there.
  [ "$status" -eq 2 ]
  [[ "$output" == *"could not reach the CSS accounts API"* ]]
}

@test "control — a row whose profile card the register SERVES is a no-op returning that webId" {
  world 401 200
  export CHORUS_IDENTITY_TOKEN="test-token"
  run "$BIN" create whole --kind agent
  [ "$status" -eq 0 ]
  [[ "$output" == *"already exists"* ]]
  [[ "${lines[0]}" == *"/whole/profile/card#me" ]]
}

@test "the same fixture flipped — ghost served, whole not — flips both verdicts" {
  # The check separates its states on the CARD STATUS, not on the name.
  world 200 401
  export CHORUS_IDENTITY_TOKEN="test-token"
  run "$BIN" create ghost --kind agent
  [ "$status" -eq 0 ]
  [[ "$output" == *"already exists"* ]]
  run "$BIN" create whole --kind agent
  [[ "$output" == *"HALF-PROVISIONED"* ]]
}

@test "plan reports HALF-PROVISIONED for a row without a pod and writes nothing" {
  world 401 200
  run "$BIN" plan ghost
  [ "$status" -eq 0 ]
  [[ "$output" == *"HALF-PROVISIONED"* ]]
}

@test "the write door's collection comes from the discovery document, not a path literal" {
  # No stub = no discovery document = no collection = refusal that names it.
  # (Store stubbed so the run reaches that step; register still dead.)
  world 401 200
  export ATHENA_MAKE_URL="http://127.0.0.1:59997" CHORUS_IDENTITY_TOKEN="test-token"
  run "$BIN" create somebody-new --kind agent
  [ "$status" -eq 2 ]
  [[ "$output" == *"discovery document does not name a Principal collection"* ]]
  ! grep -q '"/v1/identity/principals"' "${BATS_TEST_DIRNAME}/../services/chorus-principal/src/main.rs"
}

@test "the register being unreachable REFUSES before anything is written" {
  world 401 200
  export CHORUS_IDENTITY_TOKEN="test-token"
  run "$BIN" create somebody-new --kind agent
  [ "$status" -eq 2 ]
  [[ "$output" == *"could not reach the CSS accounts API"* ]]
  [[ "$output" == *"nothing was written"* ]]
}

@test "the refusal names the register as the only source of a webId" {
  # The whole card in one sentence: if the server did not give us an
  # identifier, there is nothing honest to write.
  world 401 200
  export CHORUS_IDENTITY_TOKEN="test-token"
  run "$BIN" create somebody-new --kind agent
  [[ "$output" == *"only source of a webId"* ]]
}

@test "NEGATIVE PROOF — there is no template fallback anywhere in the source" {
  # seed-css.sh:75 is the line this binary exists to delete:
  #     [ -n "$WEBID" ] || WEBID="$ISSUER_URL/$AGENT/profile/card#me"
  # It cannot fail, so it always yields a plausible WebID with nothing behind
  # it. A grep is the right shape of check: the defect is the EXISTENCE of a
  # construction path, and a behavioural test cannot prove absence.
  src="${BATS_TEST_DIRNAME}/../services/chorus-principal/src/main.rs"
  run grep -nE '^\s*(let|.*=)\s*format!\("\{issuer\}/\{name\}/profile/card' "$src"
  [ "$status" -ne 0 ]
}

@test "the plan subcommand writes nothing and says so" {
  run "$BIN" plan somebody-new
  [ "$status" -eq 0 ]
  [[ "$output" == *"REFUSE"* ]]
  [[ "$output" == *"all three, or none"* ]]
}

@test "plan is a separate verb, not a dry-run flag on create" {
  # A boolean that switches between writing and not writing is one wrong
  # default away from provisioning during a test run. This card exists because
  # identities got made by accident.
  src="${BATS_TEST_DIRNAME}/../services/chorus-principal/src/main.rs"
  run grep -cE '\-\-dry-run|dry_run' "$src"
  [ "$output" = "0" ]
}

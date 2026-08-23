#!/usr/bin/env bats
# @test-type: unit — hermetic CLI validation-path runs (dummy token, dead board URL); no live service
# #3682 — guest-cannot-authorize, enforced in code (ADR-054 norm → mechanism).
#
# The door: an authorization-bearing action (cards add with DEPLOY_ROLE=jeff —
# the "Jeff authorized this" attribution) whose ORIGINATING identity is a
# known non-Jeff human Principal must refuse. The #3679 shape: Mark asks in
# the Clearing, a role files it as owner-authorized.
#
# Origin identity travels as CHORUS_ORIGIN_PRINCIPAL (WebID), set by
# Clearing-facing surfaces from the verified CSS session (post-#3669).
# Absent origin = today's direct-terminal paths, unchanged (AC2).
#
# Test-writing invariant (#3528): brings its own world — no live board write.
# --validate-only exercises the full door without mutating Vikunja.

CARDS_DIR="$BATS_TEST_DIRNAME/../../directing/products/cards"
JEFF_WEBID="https://id.gathering.local/jeff/profile/card#me"
MARK_WEBID="https://id.gathering.local/mark/profile/card#me"

run_cards_add() {
  # Hermetic world (#3528): dummy token + dead board URL. --validate-only must
  # never reach the board, so a dead URL is itself an assertion of that.
  VIKUNJA_TOKEN=dummy VIKUNJA_URL=http://127.0.0.1:9 \
  npx --prefix "$CARDS_DIR" ts-node "$CARDS_DIR/src/cli.ts" add \
    "test: guest-authorization probe (never files)" \
    --owner silas --priority P3 --domain chorus --type chore \
    --origin reactive --sequence ops \
    --desc $'## Experience\nprobe\n\n## AC\n- [ ] probe' \
    --validate-only
}

# AC1/AC3 — the negative proof (red first): guest-originated Jeff-attribution refuses
@test "guest origin + DEPLOY_ROLE=jeff refuses with authorization-requires-Jeff" {
  DEPLOY_ROLE=jeff CHORUS_ORIGIN_PRINCIPAL="$MARK_WEBID" run run_cards_add
  [ "$status" -ne 0 ]
  [[ "$output" == *"authorization requires Jeff"* ]]
}

@test "guest refusal names the originating principal" {
  DEPLOY_ROLE=jeff CHORUS_ORIGIN_PRINCIPAL="$MARK_WEBID" run run_cards_add
  [[ "$output" == *"$MARK_WEBID"* ]]
}

# AC2 — regression: Jeff-originated path unchanged. Asserts POSITIVE success
# ("validation OK"), not just absence of the refusal — a wrong-reason failure
# (e.g. unknown flag) cannot read as a pass (#3725 bogus-fixture lesson).
@test "jeff origin + DEPLOY_ROLE=jeff passes the guest door" {
  CHORUS_JEFF_WEBID="$JEFF_WEBID" DEPLOY_ROLE=jeff CHORUS_ORIGIN_PRINCIPAL="$JEFF_WEBID" run run_cards_add
  [ "$status" -eq 0 ]
  [[ "$output" == *"validation OK"* ]]
}

# AC2 — regression: no origin claim (direct terminal, today's path) unchanged
@test "absent origin + DEPLOY_ROLE=jeff passes the guest door" {
  DEPLOY_ROLE=jeff run run_cards_add
  [ "$status" -eq 0 ]
  [[ "$output" == *"validation OK"* ]]
}

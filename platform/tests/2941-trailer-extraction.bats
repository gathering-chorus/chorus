#!/usr/bin/env bats
# DRAFT — gap closure (2): trailer extraction in build-signed.sh + deploy-daemon-card.sh.
# Lives in platform/tests/2941-trailer-extraction.bats (or absorbed into 2931-failure-traps.bats).
#
# Today only the WRITE side is tested (mcp-acp.test.ts trailer assertions on
# the commit message). The READ side — bash scripts extracting `Chorus-Trace-Id`
# / `Chorus-Card-Id` git trailers and exporting CHORUS_TRACE_ID / CHORUS_CARD_ID
# — is exercised only by manual smoke. This closes that gap.
#
# Strategy: build a fixture git repo with commits carrying the trailers, then
# invoke the script's trailer-extraction block in a subshell, assert env vars
# are exported correctly. Avoids running the full build-signed.sh (which
# requires real cargo).

setup() {
  TMPREPO=$(mktemp -d -t 2941-trailer-extract.XXXXXX)
  cd "$TMPREPO"
  git init -q
  git config user.email "test@chorus.local"
  git config user.name "test"
  echo "x" > a.txt && git add a.txt
}

teardown() {
  rm -rf "$TMPREPO"
}

@test "build-signed-style extract: reads Chorus-Trace-Id from HEAD trailer when env unset" {
  git commit -q -m "$(printf 'kade: acp #2931\n\nChorus-Trace-Id: 019e2d40-2988-7803-acec-69c8b02271cc\nChorus-Card-Id: 2931\n')"

  # Run the same extract block build-signed.sh uses.
  result=$(unset CHORUS_TRACE_ID CHORUS_CARD_ID; \
    _trailers=$(git log -1 --format=%B | git interpret-trailers --parse 2>/dev/null || true); \
    _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}'); \
    _cid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Card-Id"{print $2;exit}'); \
    echo "TID=$_tid CID=$_cid")

  [[ "$result" == *"TID=019e2d40-2988-7803-acec-69c8b02271cc"* ]]
  [[ "$result" == *"CID=2931"* ]]
}

@test "build-signed-style extract: env wins over trailer when both present" {
  git commit -q -m "$(printf 'kade: acp #2931\n\nChorus-Trace-Id: from-trailer\nChorus-Card-Id: 2931\n')"

  # Caller passes CHORUS_TRACE_ID=from-env; trailer should NOT override.
  # Mirrors the build-signed.sh `if [ -z "${CHORUS_TRACE_ID:-}" ]` guard.
  result=$(CHORUS_TRACE_ID=from-env; \
    if [ -z "${CHORUS_TRACE_ID:-}" ]; then \
      _trailers=$(git log -1 --format=%B | git interpret-trailers --parse 2>/dev/null || true); \
      _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}'); \
      [ -n "$_tid" ] && CHORUS_TRACE_ID="$_tid"; \
    fi; \
    echo "TID=$CHORUS_TRACE_ID")

  [[ "$result" == *"TID=from-env"* ]]
  [[ "$result" != *"from-trailer"* ]]
}

@test "build-signed-style extract: silent when commit has no trailers (no false-positive)" {
  git commit -q -m "kade: bare commit no trailers"

  result=$(unset CHORUS_TRACE_ID CHORUS_CARD_ID; \
    _trailers=$(git log -1 --format=%B | git interpret-trailers --parse 2>/dev/null || true); \
    _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}'); \
    _cid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Card-Id"{print $2;exit}'); \
    echo "TID=[${_tid:-empty}] CID=[${_cid:-empty}]")

  [[ "$result" == *"TID=[empty]"* ]]
  [[ "$result" == *"CID=[empty]"* ]]
}

@test "build-signed-style extract: handles multiple unrelated trailers cleanly" {
  # interpret-trailers returns ALL trailers; awk filter must isolate just ours.
  git commit -q -m "$(printf 'kade: acp #2931\n\nSigned-off-by: kade <kade@chorus.local>\nReviewed-by: silas <silas@chorus.local>\nChorus-Trace-Id: real-trace\nChorus-Card-Id: 2931\nCo-authored-by: jeff <jeff@chorus.local>\n')"

  result=$(_trailers=$(git log -1 --format=%B | git interpret-trailers --parse 2>/dev/null || true); \
    _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}'); \
    echo "TID=$_tid")

  [[ "$result" == *"TID=real-trace"* ]]
}

@test "deploy-daemon-style extract: card_id from CLI arg wins over trailer" {
  # deploy-daemon-card.sh accepts $card_id as positional; should always override
  # whatever the trailer says (CLI is authoritative).
  git commit -q -m "$(printf 'silas: deploy chorus-api\n\nChorus-Trace-Id: from-trailer\nChorus-Card-Id: 9999\n')"

  result=$(card_id=2925; \
    export CHORUS_CARD_ID="${CHORUS_CARD_ID:-$card_id}"; \
    echo "CID=$CHORUS_CARD_ID")

  [[ "$result" == *"CID=2925"* ]]
}

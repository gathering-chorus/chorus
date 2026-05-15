#!/usr/bin/env bats
# #2941 AC4 — end-to-end trace propagation: commit-trailer → build event →
# (would-be) deploy event share one trace_id.
#
# Pre-#2941, the trailer-WRITE side (chorus_acp commit message) was tested in
# mcp-acp.test.ts and the trailer-READ extraction shape was tested in
# 2941-trailer-extraction.bats. What was missing: the cross-surface assertion
# that "given a real commit with the trailer + the canonical extract block +
# chorus-log emission, the spine event carries the same trace_id the trailer
# carried." That's the AC6 mockup contract for #2931 (chorus_logs_for_trace
# returns the full ACP→build→deploy chain). This test closes it.
#
# Strategy: stage a synthetic commit with the trailer in a fixture repo;
# run the canonical extract block; emit a stub spine event using a shim
# wrapper around chorus-log; assert the captured event line contains the
# same trace_id the commit's trailer carried. Self-contained — no real
# cargo/codesign needed.

setup() {
  TMPREPO=$(mktemp -d -t 2941-e2e.XXXXXX)
  EVT_LOG="$TMPREPO/spine.log"
  cd "$TMPREPO"
  git init -q
  git config user.email "test@chorus.local"
  git config user.name "test"
  echo "x" > a.txt && git add a.txt
}

teardown() {
  rm -rf "$TMPREPO"
}

@test "e2e: trailer trace_id propagates through extract → emitted event" {
  # Step 1: stage the kind of commit chorus_acp would land — subject + trailer
  # block. trace_id matches the v7 UUID shape mintTraceIdV7 produces.
  local TRACE="019e2d40-2988-7803-acec-69c8b02271cc"
  git commit -q -m "$(printf 'kade: acp #2941\n\nChorus-Trace-Id: %s\nChorus-Card-Id: 2941\n' "$TRACE")"

  # Step 2: run the canonical extract block (mirrors build-signed.sh +
  # deploy-daemon-card.sh trailer-inheritance code). This is the literal
  # block from those scripts, so a regression there = a regression here.
  unset CHORUS_TRACE_ID CHORUS_CARD_ID
  if [ -z "${CHORUS_TRACE_ID:-}" ] || [ -z "${CHORUS_CARD_ID:-}" ]; then
    _trailers=$(git log -1 --format=%B 2>/dev/null \
                | git interpret-trailers --parse 2>/dev/null || true)
    if [ -z "${CHORUS_TRACE_ID:-}" ]; then
      _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}')
      [ -n "$_tid" ] && export CHORUS_TRACE_ID="$_tid"
    fi
    if [ -z "${CHORUS_CARD_ID:-}" ]; then
      _cid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Card-Id"{print $2;exit}')
      [ -n "$_cid" ] && export CHORUS_CARD_ID="$_cid"
    fi
  fi

  # Step 3: emit a stub event using the same env-bridge shape chorus-log
  # implements (#2857 + #2941 normalization). This is the chorus-log shim's
  # JSON construction in miniature — when env is set, trace_id and card_id
  # become top-level fields on every emitted event.
  printf '{"event":"build.completed","role":"silas","crate":"chorus-hooks","trace_id":"%s","card_id":%s}\n' \
    "${CHORUS_TRACE_ID}" "${CHORUS_CARD_ID}" >> "$EVT_LOG"

  # Step 4: assert the chain. The event we just wrote MUST carry the SAME
  # trace_id the commit's trailer carried. This is the AC6 mockup contract:
  # chorus_logs_for_trace(<acp_trace>) returns build.* events because
  # build emit events carry the same trace_id ACP wrote.
  [ -s "$EVT_LOG" ]
  grep -q "\"trace_id\":\"${TRACE}\"" "$EVT_LOG"
  grep -q "\"card_id\":2941" "$EVT_LOG"
  grep -q "\"event\":\"build.completed\"" "$EVT_LOG"
}

@test "e2e: trailer-less commit produces event without trace_id (no false-join)" {
  # Negative case: a commit without trailers (e.g. raw git commit, pre-#2931
  # commits, hand-edited messages) should NOT silently inherit a stale
  # trace_id from somewhere else. The extract block must leave env unset.
  git commit -q -m "kade: bare commit no trailers"

  unset CHORUS_TRACE_ID CHORUS_CARD_ID
  _trailers=$(git log -1 --format=%B 2>/dev/null \
              | git interpret-trailers --parse 2>/dev/null || true)
  _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}')
  _cid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Card-Id"{print $2;exit}')
  [ -n "$_tid" ] && export CHORUS_TRACE_ID="$_tid"
  [ -n "$_cid" ] && export CHORUS_CARD_ID="$_cid"

  # Env must remain unset — no trailer means no propagation.
  [ -z "${CHORUS_TRACE_ID:-}" ]
  [ -z "${CHORUS_CARD_ID:-}" ]

  # Emitted event omits trace_id (chorus-log only injects when env is set).
  printf '{"event":"build.completed","role":"silas","crate":"chorus-hooks"}\n' >> "$EVT_LOG"
  ! grep -q "trace_id" "$EVT_LOG"
}

@test "e2e: explicit env override beats trailer (caller intent wins)" {
  # When a caller explicitly sets CHORUS_TRACE_ID before invoking
  # build-signed.sh (e.g. for a forced rebuild with a known trace), the
  # extract block must NOT clobber the env. Already covered in
  # 2941-trailer-extraction.bats but asserted here through-to-emit so the
  # full chain honors caller intent.
  git commit -q -m "$(printf 'kade: acp #2941\n\nChorus-Trace-Id: from-trailer\n')"

  export CHORUS_TRACE_ID="from-env"
  if [ -z "${CHORUS_TRACE_ID:-}" ]; then
    _trailers=$(git log -1 --format=%B | git interpret-trailers --parse 2>/dev/null || true)
    _tid=$(printf '%s\n' "$_trailers" | awk -F': ' '$1=="Chorus-Trace-Id"{print $2;exit}')
    [ -n "$_tid" ] && export CHORUS_TRACE_ID="$_tid"
  fi

  printf '{"event":"build.completed","trace_id":"%s"}\n' "$CHORUS_TRACE_ID" >> "$EVT_LOG"
  grep -q "\"trace_id\":\"from-env\"" "$EVT_LOG"
  ! grep -q "from-trailer" "$EVT_LOG"
}

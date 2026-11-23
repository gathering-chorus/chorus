#!/usr/bin/env bats
# @test-type: unit — reads the generated land pipeline; no dagu, no launchd
# @domain: cicd
#
# #4821 — the crawl ran from cicd-land's success handler, so a land that merged
# and then stopped unproven at borg-verify-in-prod (dagu runs failure, then exit)
# never crawled: #4772 and #4805 on 10-31, #4925 three times on 11-23, each fixed
# by a hand kickstart. The crawl now runs from the exit handler, which dagu runs
# after every outcome. Asserts are simple commands, never `[[ ]]`.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LAND="$ROOT/platform/pipelines/cicd-land.yaml"

# prints the command of handler $2 in pipeline file $1
handler() { awk -v h="  $2:" '$0==h{getline; sub(/^    command: /,""); print; exit}' "$1"; }
# the crawl runs from exit, and from no other handler
crawls_on_every_outcome() {
  handler "$1" exit | grep -qF 'platform/scripts/crawl-detached.sh' || return 1
  for h in success failure abort; do
    if handler "$1" "$h" | grep -qF 'crawl-detached'; then return 1; fi
  done
}

@test "#4821 the land's exit handler crawls, after success, failure and unproven alike" {
  test -f "$LAND"
  handler "$LAND" exit | grep -qF 'run-finished' || { handler "$LAND" exit; return 1; }   # control: the parse found the exit handler
  crawls_on_every_outcome "$LAND" || { sed -n '/^handler_on:/,/^steps:/p' "$LAND"; return 1; }
}

@test "#4821 NEGATIVE: a land that crawls only on success (the old shape) is caught" {
  f="$BATS_TEST_TMPDIR/cicd-land.yaml"
  cat > "$f" <<'EOF'
handler_on:
  failure:
    command: 'pair routed || true'
  exit:
    command: 'run-finished ${CARD}'
  success:
    command: 'athena-land ${CARD} || { crawl-detached.sh; exit 1; }; crawl-detached.sh'
steps:
EOF
  if crawls_on_every_outcome "$f"; then echo "a success-only crawl passed"; return 1; fi
}

@test "#4821 NEGATIVE: a land with no crawl at all is caught" {
  f="$BATS_TEST_TMPDIR/cicd-land.yaml"
  printf "handler_on:\n  exit:\n    command: 'run-finished \${CARD}'\nsteps:\n" > "$f"
  if crawls_on_every_outcome "$f"; then echo "a land with no crawl passed"; return 1; fi
}

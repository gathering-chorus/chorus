#!/usr/bin/env bats
# @test-type: unit — a throwaway git origin + fake scope and a fake dagu, no live service
# @domain: pipelines — a werk v2 land deploys the model its commit carried (#4494, #4515)
# What Jeff sees: a card that lands a model change has that model in the store
# afterwards (Wren, 2026-10-10 22:48: #4496's PolicyShape never arrived), and a
# card with no model change never starts a model run. Since #4515 the model run
# is its own dagu run, athena-land, enqueued here; /cws shows it red when it fails.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  AL="$ROOT/platform/scripts/athena-land"
  T="$BATS_TEST_TMPDIR"
  git init -q --bare "$T/origin.git"
  git clone -q "$T/origin.git" "$T/home" 2>/dev/null
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "base"
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "#77 (kade) (#9)"
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "#770 (kade) (#10)"
  git -C "$T/home" push -q origin HEAD:main
  LANDED="$(git -C "$T/home" rev-parse HEAD~1)"
  # the fake dagu records every call; `status` answers the lines in $T/states,
  # one per poll (the last one repeats), like dagu status's first line
  cat > "$T/dagu" <<STUB
#!/usr/bin/env bash
echo "\$@" >> "$T/calls"
case " \$* " in
  *" enqueue "*) [ -f "$T/enqueue-fails" ] && { echo "queue refused" >&2; exit 1; }; echo "enqueued"; exit 0 ;;
  *" status "*)
    n=\$(cat "$T/polls" 2>/dev/null || echo 0); echo \$((n + 1)) > "$T/polls"
    line=\$(sed -n "\$((n + 1))p" "$T/states"); [ -n "\$line" ] || line=\$(tail -1 "$T/states")
    echo "\$line"; echo; echo "dag: athena-land"; exit 0 ;;
esac
exit 2
STUB
  chmod +x "$T/dagu"
  echo "Succeeded - 2026-10-14T10:00:00-04:00" > "$T/states"
  export CHORUS_HOME="$T/home" ATHENA_LAND_DAGU="$T/dagu" ATHENA_LAND_POLL_S=0 ATHENA_LAND_TIMEOUT_S=5
}

scope() { printf '#!/usr/bin/env bash\necho "$@" >> "%s/scoped"\n%s\n' "$T" "$1" > "$T/scope"; chmod +x "$T/scope"; export ATHENA_LAND_SCOPE_BIN="$T/scope"; }

@test "a land that carried model sources enqueues athena-land as its own dagu run" {
  scope 'echo designing/model/policy.ttl'
  run "$AL" 77 kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"1 model file(s)"* ]] || false
  grep -q "scope $T/home ${LANDED}^..${LANDED}" "$T/scoped"
  # by NAME (dagu runs canonical's copy), with the card, its role, a run id naming the card
  grep -q -- "--config $T/home/platform/config/dagu.yaml enqueue --run-id athena-land-77-[0-9]*-[0-9]* athena-land -- CARD=77 ROLE=kade$" "$T/calls" || false
  # and waited on that same run
  rid="$(grep -o 'enqueue --run-id [^ ]*' "$T/calls" | awk '{print $3}')"
  grep -q "status --run-id $rid athena-land" "$T/calls" || false
}

@test "NEGATIVE PROOF: a land with no model source never starts a model run" {
  scope 'true'
  run "$AL" 77 kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"carries no model source"* ]] || false
  [ ! -e "$T/calls" ]
}

@test "card 77 is not card 770: the landed commit is the card's own" {
  scope 'echo designing/model/x.ttl'
  run "$AL" 77 kade
  run grep -q "$(git -C "$T/home" rev-parse HEAD)" "$T/scoped"
  [ "$status" -ne 0 ]
}

@test "a land subject in the summary form is found too" {
  git -C "$T/home" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "kade: #79 — a summary (#11)"
  git -C "$T/home" push -q origin HEAD:main
  scope 'echo designing/model/x.ttl'
  run "$AL" 79 kade
  [ "$status" -eq 0 ]
  grep -q "scope $T/home $(git -C "$T/home" rev-parse HEAD)^.." "$T/scoped" || false
  grep -q "athena-land -- CARD=79 ROLE=kade" "$T/calls" || false
}

@test "no landed commit says so and starts nothing" {
  scope 'echo designing/model/x.ttl'
  run "$AL" 78 kade
  [ "$status" -eq 0 ]
  [[ "$output" == *"no landed commit for #78"* ]] || false
  [ ! -e "$T/calls" ]
}

@test "a failed scope never reads as no model: result=failed, exit 1 (#4186)" {
  scope 'echo boom >&2; exit 3'
  run "$AL" 77 kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"scope failed"* ]] || false
  [[ "$output" != *"no model source"* ]] || false
  [[ "$(tail -1 <<<"$output")" == "athena-land #77: result=failed" ]] || false
  [ ! -e "$T/calls" ]
}

# #4186 reopen — #4513's model failed at 12:05 on 2026-10-13 and its land read
# "landed": the result line is what /cws puts on the card's own line.
@test "an athena run that succeeds ends result=deployed, exit 0 — after waiting out queued and running" {
  scope 'echo designing/model/x.ttl'
  printf 'Queued - x\nRunning - x\nSucceeded - x\n' > "$T/states"
  run "$AL" 77 kade
  [ "$status" -eq 0 ]
  [ "$(cat "$T/polls")" -eq 3 ]
  [[ "$(tail -1 <<<"$output")" == "athena-land #77: result=deployed" ]] || false
}

@test "NEGATIVE PROOF: an athena run that fails ends result=failed, exit 1 — never deployed" {
  scope 'echo designing/model/x.ttl'
  printf 'Running - x\nFailed - x\n' > "$T/states"
  run "$AL" 77 kade
  [ "$status" -eq 1 ]
  [[ "$(tail -1 <<<"$output")" == "athena-land #77: result=failed" ]] || false
  [[ "$output" == *"the model did not deploy"* ]] || false
  [[ "$output" != *"result=deployed"* ]] || false
}

@test "NEGATIVE PROOF: a run that never finishes is failed at the timeout, never deployed" {
  scope 'echo designing/model/x.ttl'
  echo "Queued - x" > "$T/states"
  ATHENA_LAND_TIMEOUT_S=3 run "$AL" 77 kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"still 'queued' after 3s"* ]] || false
  [[ "$(tail -1 <<<"$output")" == "athena-land #77: result=failed" ]] || false
}

@test "NEGATIVE PROOF: an enqueue dagu refuses is result=failed, exit 1, and nothing is polled" {
  scope 'echo designing/model/x.ttl'
  touch "$T/enqueue-fails"
  run "$AL" 77 kade
  [ "$status" -eq 1 ]
  [[ "$output" == *"dagu would not enqueue athena-land: queue refused"* ]] || false
  [[ "$(tail -1 <<<"$output")" == "athena-land #77: result=failed" ]] || false
  ! grep -q " status " "$T/calls" || false
}

@test "no model source ends result=none" {
  scope 'true'
  run "$AL" 77 kade
  [[ "$(tail -1 <<<"$output")" == "athena-land #77: result=none" ]] || false
}

@test "the old door is gone: athena-land calls no chorus_athena and no chorus-mcp-call" {
  ! grep -qE 'chorus_athena|chorus-mcp-call' <(grep -v '^#' "$AL") || false
}

#!/usr/bin/env bats
# @test-type: unit — fixture view JSON and fixture runs, no live board or dagu
# @domain: cicd
# @card: 4669 · owner: wren
# What Jeff sees: `/cws <parent>` lists each child card with its board status
# and its newest run, one line each (scripts/chorus-werk-status), read from
# `cards view <id> --json` so a change to the printed view cannot empty it.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
CWS="$REPO/platform/scripts/chorus-werk-status"

cws_py() {
  python3 - "$CWS" "$1" <<'EOF'
import importlib.machinery, importlib.util, sys
loader = importlib.machinery.SourceFileLoader("cws", sys.argv[1])
spec = importlib.util.spec_from_loader("cws", loader)
cws = importlib.util.module_from_spec(spec)
loader.exec_module(cws)
exec(sys.argv[2])
EOF
}

@test "a parent's view --json gives its children with their status" {
  run cws_py "print(cws.children_from_json('{\"index\": 4652, \"children\": [{\"index\": 4667, \"status\": \"Done\", \"title\": \"counts\"}, {\"index\": 4670, \"status\": \"WIP\", \"title\": \"reader\"}]}'))"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$output" = "[('4667', 'Done', 'counts'), ('4670', 'WIP', 'reader')]" ]
}

@test "a card with no children gives none" {
  run cws_py "print(cws.children_from_json('{\"index\": 10, \"children\": []}'))"
  [ "$output" = "[]" ]
}

@test "negative proof: output that is not a card reads unreadable (None), never 'no children'" {
  run cws_py "print(cws.children_from_json('ERROR: No task #99999 on gathering board'), cws.children_from_json('[1]'))"
  echo "$output"
  [ "$output" = "None None" ]
}

@test "each child gets one line with its newest run" {
  run cws_py "
runs = {'4667': [{'params': 'CARD=4667 ROLE=wren', 'statusLabel': 'succeeded', 'dagRunId': 'r2', 'name': 'cicd'}], '4670': []}
print('\n'.join(cws.child_lines([('4667', 'Done', 'counts'), ('4670', 'WIP', 'reader')], lambda c: runs[c])))"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "${lines[0]}" == "  #4667 [Done] counts — "* ]] || return 1
  [[ "${lines[0]}" == *"1 run"* ]] || return 1
  [ "${lines[1]}" = "  #4670 [WIP] reader — no run yet" ]
}

@test "negative proof: dagu unreadable says so, never 'no run yet'" {
  run cws_py "print(cws.child_lines([('4667', 'Done', 'counts')], lambda c: None)[0])"
  echo "$output"
  [ "$output" = "  #4667 [Done] counts — dagu unreadable" ]
}

#!/usr/bin/env bats
# @test-type: integration — reads the Skill rows through the door (:3360)
# @domain: heralds
# @card: 4585 · owner: kade (pair: borg-queen)
# What Jeff sees: a land's files, tests, services and machines show up in the
# graph the same day, because the crawl jobs run.
#
# From 10-14 to 10-19 all three crawl LaunchAgents failed every run: each called a tool
# name the runner did not offer, and crawl-nightly called an owner-only tool as
# another role. The failure lived in log files nobody read. This test reads
# every plist that calls chorus-mcp-call.sh and asks the door (the runner's own
# source of tools) whether the runner would offer that tool and accept that
# caller, by the runner's own rules (chorus-runner/src/skills.rs):
#   - no Skill row → not offered
#   - executor not deterministic | http, or no inputShape → not a tool at all
#   - an inputShape with no schema in chorus-runner/src/tool_inputs.json → not offered
#   - no implementedBy, or runsAs not caller | owner | owner-any-caller → not offered
#   - runsAs owner → only the owner and jeff may call it
# The door not answering is UNMEASURED and fails, never green.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
DOOR="${CHORUS_OWL_API:-http://localhost:3360}"
INPUTS="$REPO/platform/services/chorus-runner/src/tool_inputs.json"

# Prints "<plist>|<role>|<tool>" for each chorus-mcp-call.sh call in the plists under $1.
calls() {
  python3 - "$1" <<'PY'
import plistlib, re, sys, pathlib
for p in sorted(pathlib.Path(sys.argv[1]).glob("*.plist")):
    try:
        args = plistlib.loads(p.read_bytes()).get("ProgramArguments", [])
    except Exception as e:
        print(f"{p.name}|PARSE|{e}"); continue
    for m in re.finditer(r"chorus-mcp-call\.sh\s+'?([A-Za-z0-9_-]+)'?\s+'?([A-Za-z0-9_-]+)", " ".join(args)):
        print(f"{p.name}|{m.group(1)}|{m.group(2)}")
PY
}

# Prints the door's answer for one tool as "<http code>\n<body>". Tests replace it with a fixture.
skill_row() {
  curl -s -m 10 -w '\n%{http_code}' "$DOOR/v1/skills/skills/$1" | python3 -c 'import sys; *b, c = sys.stdin.read().split("\n"); print(c); print("\n".join(b))'
}

# Prints one problem per call the runner would refuse; nothing when every call runs.
check() {
  local plist role tool answer code
  while IFS='|' read -r plist role tool; do
    test -n "$plist" || continue
    if test "$role" = PARSE; then echo "$plist: does not parse: $tool"; continue; fi
    answer=$(skill_row "$tool")
    code=${answer%%$'\n'*}
    case "$code" in
      200) ;;
      404) echo "$plist: tool $tool is not a Skill row the runner offers"; continue ;;
      *) echo "UNMEASURED: the door answered '$code' for $tool"; continue ;;
    esac
    printf '%s' "${answer#*$'\n'}" | python3 -c '
import json, sys
plist, role, tool, inputs = sys.argv[1:]
row = json.load(sys.stdin)
d, links = row.get("data", {}), row.get("links", {})
bare = lambda v: (v or "").rsplit("#", 1)[-1].rsplit("/", 1)[-1].rsplit(":", 1)[-1].removeprefix("principal-")
runs, owner, shape = bare(d.get("runsAs")), bare(links.get("ownedBy")), bare(d.get("inputShape") or links.get("inputShape"))
ex = d.get("executor")
if ex not in ("deterministic", "http"):
    print(f"{plist}: tool {tool} has executor {ex!r}, so the runner does not offer it")
elif not shape:
    print(f"{plist}: tool {tool} names no inputShape, so the runner does not offer it")
elif not isinstance(json.load(open(inputs)).get(shape), dict):
    print(f"{plist}: tool {tool} input shape {shape} has no generated schema, so the runner does not offer it")
elif not d.get("implementedBy"):
    print(f"{plist}: tool {tool} names no binary (implementedBy), so the runner does not offer it")
elif runs not in ("caller", "owner", "owner-any-caller"):
    print(f"{plist}: tool {tool} has runsAs {runs!r}, so the runner does not offer it")
elif runs == "owner" and role not in (owner, "jeff"):
    print(f"{plist}: {role} calls {tool}, which runs only as its owner {owner}")
' "$plist" "$role" "$tool" "$INPUTS"
  done < <(calls "$1")
}

# A fixture plist under $BATS_TEST_TMPDIR/$1 calling tool $3 as role $2.
plist_calling() {
  mkdir -p "$BATS_TEST_TMPDIR/$1"
  cat > "$BATS_TEST_TMPDIR/$1/x.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>ProgramArguments</key><array>
<string>/bin/sh</string><string>-c</string><string>/x/chorus-mcp-call.sh $2 $3 '{}'</string>
</array></dict></plist>
PLIST
}

# A fixture door answer: 200 and a Skill row with the given runsAs, implementedBy, owner,
# and optionally executor (default deterministic) and inputShape (default HeraldRunInput).
fixture_row() {
  printf '200\n{"data":{"runsAs":"%s","implementedBy":"%s","executor":"%s","inputShape":"%s"},"links":{"ownedBy":"chorus:principal-%s"}}' \
    "$1" "$2" "${4-deterministic}" "${5-HeraldRunInput}" "$3"
}

@test "every LaunchAgent that calls a tool names one the runner offers, as a caller it accepts" {
  curl -s -m 5 -o /dev/null "$DOOR/health" || { echo "UNMEASURED: the door at $DOOR does not answer"; return 1; }
  test "$(calls "$REPO/platform/launchd" | wc -l)" -gt 0 || { echo "no plist calls chorus-mcp-call.sh: the guard reads nothing"; return 1; }
  run check "$REPO/platform/launchd"
  echo "$output"
  test -z "$output"
}

@test "the three crawl LaunchAgents are among the plists checked" {
  out=$(calls "$REPO/platform/launchd" | cut -d'|' -f1 | sort -u)
  for p in crawl-nightly infra-stores-crawl service-harvest; do
    printf '%s\n' "$out" | grep -qx "com.chorus.$p.plist" || { echo "com.chorus.$p.plist is not read: $out"; return 1; }
  done
}

@test "NEGATIVE: a plist naming a tool with no Skill row is caught (live door)" {
  curl -s -m 5 -o /dev/null "$DOOR/health" || { echo "UNMEASURED: the door at $DOOR does not answer"; return 1; }
  plist_calling bad silas no-such-tool-4585
  run check "$BATS_TEST_TMPDIR/bad"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c 'is not a Skill row')" -eq 1
}

@test "NEGATIVE: a plist calling an owner-only tool as another role is caught; the owner and jeff pass" {
  skill_row() { fixture_row owner chorus-crawl silas; }
  plist_calling other kade some-tool
  run check "$BATS_TEST_TMPDIR/other"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c 'kade calls some-tool, which runs only as its owner silas')" -eq 1
  for ok in silas jeff; do
    plist_calling "$ok" "$ok" some-tool
    run check "$BATS_TEST_TMPDIR/$ok"
    test -z "$output" || { echo "$ok was refused: $output"; return 1; }
  done
}

@test "NEGATIVE: a tool with no runsAs, or no binary, is caught as not offered" {
  plist_calling none silas some-tool
  skill_row() { fixture_row "" chorus-crawl silas; }
  run check "$BATS_TEST_TMPDIR/none"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c "has runsAs ''")" -eq 1
  skill_row() { fixture_row caller "" silas; }
  run check "$BATS_TEST_TMPDIR/none"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c 'names no binary')" -eq 1
}

@test "NEGATIVE: a prompt Skill, a tool with no inputShape, or one whose shape has no schema is caught" {
  plist_calling shape silas some-tool
  skill_row() { fixture_row caller chorus-crawl silas agent; }
  run check "$BATS_TEST_TMPDIR/shape"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c "has executor 'agent'")" -eq 1
  skill_row() { fixture_row caller chorus-crawl silas deterministic ""; }
  run check "$BATS_TEST_TMPDIR/shape"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c 'names no inputShape')" -eq 1
  skill_row() { fixture_row caller chorus-crawl silas http NoSuchInput4585; }
  run check "$BATS_TEST_TMPDIR/shape"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c 'NoSuchInput4585 has no generated schema')" -eq 1
  # the fixture's default shape is a real one, so the clean fixture passes
  skill_row() { fixture_row caller chorus-crawl silas; }
  run check "$BATS_TEST_TMPDIR/shape"
  test -z "$output" || { echo "a clean fixture was refused: $output"; return 1; }
}

@test "NEGATIVE: the door not answering is UNMEASURED, never clean" {
  skill_row() { printf '000\n'; }
  plist_calling down silas herald-run
  run check "$BATS_TEST_TMPDIR/down"
  echo "$output"
  test "$(printf '%s\n' "$output" | grep -c '^UNMEASURED')" -eq 1
}

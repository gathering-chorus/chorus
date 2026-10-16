#!/usr/bin/env bats
# @test-type: unit
# @domain: services
# 4515-no-underscore-tool-names.bats — #4515. Tools are served by chorus-runner
# from Skill rows, named by Jeff's rule (2026-10-14): chorus-<product> starts a
# pipeline, <product>-<step>[-<object>] runs one step. Every old name must be
# gone from anything an agent, hook, script or permission reads:
#   - the underscore names of the old MCP server (chorus_nudge_message, ...),
#   - the hyphen names they had for a day (chorus-nudge-message, herald-run, ...),
#   - the proving and deploy rows' old names (prove-intake, deploy-demo, ...),
#   - the retired tools (priorities readout, commit status, design refresh, ...).
# History (html, jsonl, briefs, journals, ADRs, notes, backups, logs,
# activity.md, consumed next-session files) is left as it was written.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SELF="$(basename "$BATS_TEST_FILENAME")"

# The old names, underscore and hyphen spellings. chorus-werk and chorus-athena
# are the NEW pipeline starters, so only their underscore spellings are old.
# deploy-demo / deploy-prod are old only as a tool (skill-…, `call …`, mcp__…):
# /cws shortens werk-deploy-prod to deploy-prod on its board line (Jeff's view).
OLD='chorus[_-](nudge[_-]message|cards[_-](add|set|view|move|done|tag)|card[_-]add[_-]jeff|wip|sup|logs[_-](for[_-](card|trace|branch)|recent[_-]errors)|flow[_-]report|blast[_-]radius|pain[_-](rollup|card)|service[_-](status|start|stop|restart|deploy|rollback)|decisions[_-](list|get)|principles[_-](list|get|create)|priorities[_-]readout|commit[_-]status|unpull[_-]card|design[_-]refresh|register[_-]feedback)|chorus_(werk|athena)|herald[_-]run|prove-(intake|verify|stage|verdict|in-prod|record)|(skill-|call |mcp__chorus-api__)deploy-(demo|prod)'

# Prints file:match for every old name in the files git tracks under $1, bare or
# behind an MCP server prefix (mcp__chorus-api__<name>). Tracked files only (Silas,
# 2026-10-16): an untracked deploy copy (platform/mcp-server/dist.prev, the 10-16
# nightly red) or build output is out by construction, not by an exclude list.
# Exit 0 always; the caller reads the output.
old_tool_names() {
  local root="$1" x='(exclude,glob)'
  git -C "$root" grep -nIE "(^|[^a-z0-9_-]|__|\\\\[tnr])(${OLD})([^a-z0-9_-]|\$)" -- . \
    ":$x**/*.html" ":$x**/*.jsonl" ":$x**/activity.md" ":$x**/*.consumed" ":$x**/$SELF" \
    ":$x**/node_modules/**" ":$x**/target/**" ":$x**/briefs/**" ":$x**/journal/**" \
    ":$x**/journals/**" ":$x**/adr/**" ":$x**/notes/**" ":$x**/backups/**" ":$x**/logs/**" \
    2>/dev/null | code_comments_dropped || true
}

# A scan dir for the negative proofs: a git repo whose files are tracked, because
# the check reads tracked files only.
track() { git -C "$1" init -q 2>/dev/null; git -C "$1" add -A; }

# A name inside a code comment calls nothing (#4515 run 2: two old test files
# kept a comment naming the old nudge tool; changing them only to reword the
# comment makes werk-test refuse a change no test exercises). A hit is dropped
# only when its whole line is a comment in a code file (.ts .js .sh .bats .rs
# .py): `//`, `#`, `*`, `/*` first. Markdown, TOML, JSON and plists are prose or
# config an agent or job reads, so every hit there stays.
code_comments_dropped() {
  awk '{
    f=$0; sub(/:.*/, "", f)
    line=$0; sub(/^[^:]*:[0-9]+:/, "", line)
    if (f ~ /\.(ts|js|sh|bats|rs|py)$/ && line ~ /^[ \t]*(\/\/|#|\*|\/\*)/) next
    print
  }'
}

@test "no tool is called by an old name" {
  [ -d "$REPO/platform" ] || { echo "guard target missing: $REPO/platform"; false; }
  hits="$(old_tool_names "$REPO")"
  [[ -z "$hits" ]] || { echo "old tool names (rename per #4515):"; echo "$hits"; false; }
}

@test "NEGATIVE PROOF: an old underscore name planted in a scan dir is caught" {
  scan="$BATS_TEST_TMPDIR/scan"
  mkdir -p "$scan/skills/x"
  # built from parts so this file never names the old tool itself
  printf 'call mcp__chorus-api__%s({ role: "wren" })\n' "chorus""_wip" > "$scan/skills/x/SKILL.md"
  track "$scan"; hits="$(old_tool_names "$scan")"
  echo "caught: $hits"
  [[ "$hits" == *"chorus""_wip"* ]] || { echo "check missed a planted old name"; false; }
}

@test "NEGATIVE PROOF: an old hyphen name, bare or prefixed, is caught" {
  scan="$BATS_TEST_TMPDIR/hyphen"
  mkdir -p "$scan/platform/scripts" "$scan/skills/y"
  printf 'chorus-mcp-call.sh silas %s {}\n' "herald""-run" > "$scan/platform/scripts/job.sh"
  printf 'chorus-runner call %s card=1\n' "deploy""-prod" >> "$scan/platform/scripts/job.sh"
  printf 'mcp__chorus-api__%s({ to: "kade" })\n' "chorus-nudge""-message" > "$scan/skills/y/SKILL.md"
  printf 'const ROWS: &str = "intake\\t1\\t%s\\tdeterministic";\n' "prove""-intake" > "$scan/platform/scripts/rows.rs"
  track "$scan"; hits="$(old_tool_names "$scan")"
  echo "caught: $hits"
  [[ "$hits" == *"herald""-run"* ]] || { echo "missed a bare old hyphen name"; false; }
  [[ "$hits" == *"deploy""-prod"* ]] || { echo "missed an old deploy row name"; false; }
  [[ "$hits" == *"chorus-nudge""-message"* ]] || { echo "missed a prefixed old hyphen name"; false; }
  [[ "$hits" == *"prove""-intake"* ]] || { echo "missed an old name after a \\t escape in a string"; false; }
}

@test "NEGATIVE PROOF: the new names, the chorus-werk directory and history pass" {
  scan="$BATS_TEST_TMPDIR/clean"
  mkdir -p "$scan/skills/x" "$scan/roles/wren/briefs"
  printf 'mcp__chorus-api__clearing-nudge, mcp__chorus-api__chorus-werk, borg-tend-herald\n' > "$scan/skills/x/SKILL.md"
  printf 'werk-deploy-demo werk-deploy-prod borg-verify-in-prod clearing-wip ~/CascadeProjects/chorus-werk/wren-1\n' >> "$scan/skills/x/SKILL.md"
  printf 'mcp__chorus-api__%s\n' "chorus""_wip" > "$scan/roles/wren/briefs/old.md"
  track "$scan"; hits="$(old_tool_names "$scan")"
  [[ -z "$hits" ]] || { echo "false fire: $hits"; false; }
}

@test "NEGATIVE PROOF: a comment line in code passes, the same name in a call does not" {
  scan="$BATS_TEST_TMPDIR/comments"
  mkdir -p "$scan/platform/tests" "$scan/skills/z"
  n="chorus""_nudge_message"
  printf '// delivered via the %s tool\n' "$n" > "$scan/platform/tests/steps.ts"
  printf '# via the %s MCP tool\n' "$n" > "$scan/platform/tests/inject.sh"
  track "$scan"; hits="$(old_tool_names "$scan")"
  [[ -z "$hits" ]] || { echo "a comment fired: $hits"; false; }
  printf 'await call("%s", {});\n' "$n" >> "$scan/platform/tests/steps.ts"
  printf 'the %s tool\n' "$n" > "$scan/skills/z/SKILL.md"
  track "$scan"; hits="$(old_tool_names "$scan")"
  echo "caught: $hits"
  [[ "$hits" == *"steps.ts:2:"* ]] || { echo "missed a call in code"; false; }
  [[ "$hits" == *"SKILL.md"* ]] || { echo "missed prose an agent reads"; false; }
  [[ "$hits" != *"inject.sh"* ]] || { echo "comment in a script fired"; false; }
}

@test "NEGATIVE PROOF: an untracked copy is out, the same file tracked is caught" {
  scan="$BATS_TEST_TMPDIR/untracked"
  mkdir -p "$scan/platform/mcp-server/dist.prev"
  git -C "$scan" init -q
  printf "name: '%s',\n" "chorus""_principles_list" > "$scan/platform/mcp-server/dist.prev/server.js"
  hits="$(old_tool_names "$scan")"
  [[ -z "$hits" ]] || { echo "an untracked deploy copy fired: $hits"; false; }
  git -C "$scan" add -A
  hits="$(old_tool_names "$scan")"
  [[ "$hits" == *"dist.prev/server.js"* ]] || { echo "missed the name once tracked"; false; }
}

#!/usr/bin/env bats
# @test-type: unit
# @domain: services
# 4515-no-underscore-tool-names.bats — #4515. Tools are served by chorus-runner
# from Skill rows, and a Skill name is lowercase-hyphen. The renamed tools
# (chorus_nudge_message → chorus-nudge-message, herald_run → herald-run, ...)
# must not be called by their old underscore names anywhere an agent, hook or
# permission would read them. History (html, jsonl, briefs, journals, ADRs,
# notes, backups, logs, activity.md) are left as they were written.
#
# The tools whose rename is still undecided (chorus_werk, chorus_athena, ...)
# are not in RENAMED, so they do not trip this guard.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"

RENAMED='chorus_(nudge_message|cards_add|card_add_jeff|cards_done|cards_view|cards_move|cards_set|cards_tag|wip|sup|logs_for_card|logs_for_trace|logs_recent_errors|decisions_list|decisions_get|principles_list|pain_rollup|flow_report|blast_radius|service_(status|start|stop|restart|deploy|rollback))|herald_run'

# Prints file:name for every old underscore tool name under $1, prefixed the
# way an MCP client calls it. Exit 0 always; the caller reads the output.
old_tool_names() {
  local root="$1"
  local prefix='mcp__chorus-api__'
  grep -rIoE \
    --exclude='*.html' --exclude='*.jsonl' --exclude='activity.md' \
    --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target \
    --exclude-dir=briefs --exclude-dir=journal \
    --exclude-dir=journals --exclude-dir=adr --exclude-dir=notes \
    --exclude-dir=backups --exclude-dir=logs \
    "${prefix}(${RENAMED})([^a-z0-9_]|\$)" "$root" 2>/dev/null || true
}

@test "no renamed tool is called by its old underscore name" {
  [ -d "$REPO/platform" ] || { echo "guard target missing: $REPO/platform"; false; }
  hits="$(old_tool_names "$REPO")"
  [[ -z "$hits" ]] || { echo "old underscore tool names (rename to hyphen, #4515):"; echo "$hits"; false; }
}

@test "NEGATIVE PROOF: the check catches an old name planted in a scan dir" {
  scan="$BATS_TEST_TMPDIR/scan"
  mkdir -p "$scan/skills/x"
  # built from parts so this file never names the old tool itself
  printf 'call mcp__chorus-api__%s({ role: "wren" })\n' "chorus_wip" > "$scan/skills/x/SKILL.md"
  hits="$(old_tool_names "$scan")"
  echo "caught: $hits"
  [[ "$hits" == *"mcp__chorus-api__chorus""_wip"* ]] || { echo "check missed a planted old name"; false; }
}

@test "NEGATIVE PROOF: the new name, an undecided name and history pass" {
  scan="$BATS_TEST_TMPDIR/clean"
  mkdir -p "$scan/skills/x" "$scan/roles/wren/briefs"
  printf 'mcp__chorus-api__chorus-wip and mcp__chorus-api__%s\n' "chorus_werk" > "$scan/skills/x/SKILL.md"
  printf 'mcp__chorus-api__%s\n' "chorus_wip" > "$scan/roles/wren/briefs/old.md"
  hits="$(old_tool_names "$scan")"
  [[ -z "$hits" ]] || { echo "false fire: $hits"; false; }
}

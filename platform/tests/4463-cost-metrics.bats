#!/usr/bin/env bats
# @test-type: unit — fixture transcripts, no live service
# @domain: observability — the cost dashboard's exporter (#4463)
# 4463-cost-metrics.bats — what Jeff sees: the cost dashboard shows this month's
# Claude tokens per role, Abby's Gemini spend, and says UNMEASURED when a source
# can't be read. Before #4463 it globbed folders that no longer exist and showed
# 0 tokens as if measured.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/cost-metrics"
  T="$BATS_TEST_TMPDIR"
  export COST_CLAUDE_PROJECTS="$T/claude" COST_GEMINI_TMP="$T/gemini" COST_TODAY="2026-10-09"
  export COST_CONFIG="$ROOT/platform/config/cost.json"
  unset TWILIO_ACCOUNT_SID TWILIO_AUTH_TOKEN
  mkdir -p "$T/claude" "$T/gemini"
}

# claude_row <project-dir> <msg-id> <timestamp> <output-tokens>
claude_row() {
  mkdir -p "$T/claude/$1"
  printf '{"timestamp":"%s","sessionId":"s-%s","requestId":"r-%s","message":{"id":"%s","usage":{"input_tokens":1,"output_tokens":%s,"cache_read_input_tokens":10,"cache_creation_input_tokens":0}}}\n' \
    "$3" "$1" "$2" "$2" "$4" >> "$T/claude/$1/session.jsonl"
}

# gemini_row <model> <id> <timestamp> <input> <output>
gemini_row() {
  mkdir -p "$T/gemini/abby-normal-1/chats"
  printf '{"id":"%s","timestamp":"%s","type":"gemini","model":"%s","tokens":{"input":%s,"output":%s,"cached":0,"thoughts":0,"tool":0,"total":0}}\n' \
    "$2" "$3" "$1" "$4" "$5" >> "$T/gemini/abby-normal-1/chats/session.jsonl"
}

# no output line matches $1 (a plain `! grep` cannot fail a bats test on bash 3.2, #4335)
lacks() { if printf '%s\n' "$output" | grep -q -- "$1"; then echo "unexpected: $1"; return 1; fi; }

# the value of one sample line, never a HELP/TYPE line that names the metric
metric() { printf '%s\n' "$output" | awk -v k="$1" 'index($0, k " ") == 1 {print $NF}'; }

@test "Claude tokens are counted per role from today's chorus folders, each response once" {
  claude_row -Users-x-CascadeProjects-chorus-roles-silas m1 2026-10-09T13:00:00Z 100
  claude_row -Users-x-CascadeProjects-chorus-roles-silas m1 2026-10-09T13:00:00Z 100   # same response logged twice
  claude_row -Users-x-CascadeProjects-chorus-werk-kade-4454 m2 2026-10-09T13:00:00Z 40
  claude_row -var-folders-4j-abc-T-run m3 2026-10-09T13:00:00Z 7
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [ "$(metric 'cost_source_measured{source="claude"}')" = 1 ]
  [ "$(metric 'claude_role_output_tokens{role="silas"}')" = 100 ]
  [ "$(metric 'claude_role_output_tokens{role="kade"}')" = 40 ]
  [ "$(metric 'claude_role_output_tokens{role="headless"}')" = 7 ]
  [ "$(metric claude_billing_output_tokens)" = 147 ]
}

@test "last month's responses are not this month's" {
  claude_row -Users-x-CascadeProjects-chorus-roles-wren m1 2026-09-30T12:00:00Z 500
  claude_row -Users-x-CascadeProjects-chorus-roles-wren m2 2026-10-01T12:00:00Z 5
  run "$SCRIPT"
  [ "$(metric 'claude_role_output_tokens{role="wren"}')" = 5 ]
}

@test "negative proof: no transcripts reads UNMEASURED, not 0 tokens" {
  rm -rf "$T/claude"
  run "$SCRIPT"
  [ "$status" -eq 0 ]
  [ "$(metric 'cost_source_measured{source="claude"}')" = 0 ]
  lacks '^claude_billing_output_tokens'
  lacks '^claude_role_'
}

@test "Abby's Gemini spend is tokens times the price; an unpriced model is unmeasured, not \$0" {
  gemini_row gemini-3.8-flash g1 2026-10-09T13:00:00Z 1000000 100000
  gemini_row gemini-3.5-flash-lite g2 2026-10-09T13:00:00Z 5000 50
  run "$SCRIPT"
  [ "$(metric 'cost_source_measured{source="gemini"}')" = 1 ]
  # 1M input x $0.75 + 0.1M output x $3.75 = $1.125
  [ "$(metric 'cost_gemini_dollars{model="gemini-3.8-flash"}')" = 1.1250 ]
  [ "$(metric 'cost_gemini_spend_measured{model="gemini-3.5-flash-lite"}')" = 0 ]
  lacks 'cost_gemini_dollars{model="gemini-3.5-flash-lite"}'
  [ "$(metric cost_total_complete)" = 0 ]
}

@test "the half-price window ends: after valid_until the after price applies" {
  export COST_TODAY="2027-01-05"
  gemini_row gemini-3.8-flash g1 2027-01-05T13:00:00Z 1000000 0
  run "$SCRIPT"
  [ "$(metric 'cost_gemini_dollars{model="gemini-3.8-flash"}')" = 1.5000 ]
}

@test "negative proof: no Gemini chats and no Twilio credentials are unmeasured and the total says incomplete" {
  rm -rf "$T/gemini"
  run "$SCRIPT"
  [ "$(metric 'cost_source_measured{source="gemini"}')" = 0 ]
  [ "$(metric 'cost_source_measured{source="twilio"}')" = 0 ]
  lacks '^cost_twilio_sms_dollars'
  [ "$(metric cost_fixed_claude_dollars)" = 200 ]
  [ "$(metric cost_total_complete)" = 0 ]
}

@test "the old Gathering-era folder is not counted as a role" {
  claude_row -Users-x-CascadeProjects-architect m1 2026-10-09T13:00:00Z 9
  run "$SCRIPT"
  [ "$(metric 'claude_role_output_tokens{role="other"}')" = 9 ]
  lacks 'role="silas"'
}

@test "every metric the dashboard asks for is one the exporter writes" {
  run python3 - "$ROOT" <<'PY'
import json, re, sys
root = sys.argv[1]
d = json.load(open(f"{root}/dashboards/cost-dashboard.json"))
src = open(f"{root}/platform/scripts/cost-metrics").read()
used = {m for p in d["panels"] for t in p.get("targets", []) for m in re.findall(r"\b[a-z]+_[a-z_]+\b", t.get("expr", ""))}
missing = sorted(m for m in used if f'"{m}"' not in src)
print("missing:", missing)
sys.exit(1 if missing else 0)
PY
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

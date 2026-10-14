#!/usr/bin/env bats
# @test-type: fitness — static repo scan, no live service
# @domain: services — the tool runner is the only place a tool is made (#4515)
# 4515-no-hand-written-tools.bats — regression guard (#4515 AC3)
# What Jeff sees: every tool a session or job calls comes from a Skill row in
# the model. Nobody can slip a hand-written MCP tool back in beside the
# runner: the 4,211-line platform/mcp-server was that, and it took five cards
# to retire. A tool is made in exactly one place, chorus-runner's tools(),
# from three sources: door collections, Skill rows, and task_status.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SELF="platform/tests/$(basename "$BATS_TEST_FILENAME")"
RUNNER_SERVER="platform/services/chorus-runner/src/server.rs"

# What it does NOT catch: a server that hand-rolls JSON-RPC with no SDK and
# no rmcp at all. Every tool server we have had used one of the two.
# Code lines only (a Rust #[attribute] is code, not a # comment): a comment that names a pattern is prose, not a tool.
code_hits() {
  grep -nE "$2" "$1" 2>/dev/null | grep -vE '^[0-9]+:[[:space:]]*(//|#([^[]|$)|\*|/\*)' || true
}

# The files a scan covers: tracked files in a git repo, every file in a fixture.
files_in() {
  if git -C "$1" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$1" ls-files
  else
    (cd "$1" && find . -type f -not -path '*/node_modules/*' | sed 's#^\./##')
  fi
}

# Prints one line per violation; prints nothing when the tree is clean.
scan() {
  local root="$1" f n
  [ -f "$root/$RUNNER_SERVER" ] || { echo "MISSING $RUNNER_SERVER: the guard's anchor is gone"; return; }
  while IFS= read -r f; do
    [ "$f" = "$SELF" ] && continue
    case "$f" in
      */node_modules/*|*/target/*) continue ;;
      *package.json)
        [ -n "$(code_hits "$root/$f" '"@modelcontextprotocol/sdk"')" ] && echo "SDK dependency: $f" ;;
      *.ts|*.js|*.mjs|*.py|*.rs)
        [ "$f" = "$RUNNER_SERVER" ] && continue
        code_hits "$root/$f" 'registerTool\(|\.tool\([[:space:]]*["'\'']|new McpServer\(|ListToolsRequestSchema|CallToolRequestSchema|Tool::new\(|#\[tool(_router)?(\(|\])|async fn list_tools\(|@mcp\.tool' \
          | sed "s#^#hand-written tool: $f:#" ;;
    esac
  done < <(files_in "$root")
  # Inside the runner: exactly the three sources. A fourth Tool::new is a hand-written tool.
  n=$(code_hits "$root/$RUNNER_SERVER" 'Tool::new\(' | wc -l | tr -d ' ')
  [ "$n" -eq 3 ] || echo "runner makes $n tools by hand-count, expected 3 (door, skills, task_status): $RUNNER_SERVER"
}

# A minimal tree that passes: the runner's three sources and nothing else.
clean_fixture() {
  local d="$1"
  mkdir -p "$d/platform/services/chorus-runner/src" "$d/platform/api"
  cat > "$d/$RUNNER_SERVER" <<'RS'
fn tools() {
    let a = Tool::new(t.name, t.description, s);
    let b = Tool::new(t.name, t.description, s);
    let c = Tool::new("task_status", "d", s);
}
RS
  echo '{ "dependencies": { "express": "4.19.2" } }' > "$d/platform/api/package.json"
  echo '// a comment may say registerTool( without being one' > "$d/platform/api/ok.ts"
}

@test "the repo has no hand-written MCP tool and no MCP SDK dependency" {
  out=$(scan "$REPO")
  [ -z "$out" ] || { echo "$out"; false; }
}

@test "a clean fixture passes (the scan can go green)" {
  d="$BATS_TEST_TMPDIR/clean"; clean_fixture "$d"
  out=$(scan "$d")
  [ -z "$out" ] || { echo "$out"; false; }
}

@test "NEGATIVE PROOF: a TypeScript registerTool goes red" {
  d="$BATS_TEST_TMPDIR/ts"; clean_fixture "$d"
  echo 'server.registerTool("chorus_sneaky", {}, async () => ({}));' > "$d/platform/api/sneaky.ts"
  out=$(scan "$d")
  [[ "$out" == *"hand-written tool: platform/api/sneaky.ts"* ]] || return 1
}

@test "NEGATIVE PROOF: the old server's own shape (setRequestHandler ListTools) goes red" {
  d="$BATS_TEST_TMPDIR/old"; clean_fixture "$d"
  printf 'server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: [{ name: "chorus_x" }] }));\n' > "$d/platform/api/old.ts"
  out=$(scan "$d")
  [[ "$out" == *"hand-written tool: platform/api/old.ts"* ]] || return 1
}

@test "NEGATIVE PROOF: an rmcp #[tool] outside the runner goes red" {
  d="$BATS_TEST_TMPDIR/rs"; clean_fixture "$d"
  mkdir -p "$d/platform/services/other/src"
  printf '#[tool(description = "x")]\nasync fn sneaky() {}\n' > "$d/platform/services/other/src/lib.rs"
  out=$(scan "$d")
  [[ "$out" == *"hand-written tool: platform/services/other/src/lib.rs"* ]] || return 1
}

@test "NEGATIVE PROOF: a fourth Tool::new inside the runner goes red" {
  d="$BATS_TEST_TMPDIR/runner"; clean_fixture "$d"
  printf '    let x = Tool::new("chorus_sneaky", "d", s);\n' >> "$d/$RUNNER_SERVER"
  out=$(scan "$d")
  [[ "$out" == *"runner makes 4 tools"* ]] || return 1
}

@test "NEGATIVE PROOF: an MCP SDK dependency goes red" {
  d="$BATS_TEST_TMPDIR/sdk"; clean_fixture "$d"
  echo '{ "dependencies": { "@modelcontextprotocol/sdk": "1.31.0" } }' > "$d/platform/api/package.json"
  out=$(scan "$d")
  [[ "$out" == *"SDK dependency: platform/api/package.json"* ]] || return 1
}

@test "a client that lists tools is not a tool server" {
  d="$BATS_TEST_TMPDIR/client"; clean_fixture "$d"
  mkdir -p "$d/platform/services/hooks/src"
  printf 'pub fn list_tools(session: &McpSession) -> Vec<Tool> { call(session, "tools/list") }\n' > "$d/platform/services/hooks/src/mcp_client.rs"
  out=$(scan "$d")
  [ -z "$out" ] || { echo "$out"; false; }
}

@test "NEGATIVE PROOF: an rmcp server's list_tools outside the runner goes red" {
  d="$BATS_TEST_TMPDIR/rmcp"; clean_fixture "$d"
  mkdir -p "$d/platform/services/other/src"
  printf 'impl ServerHandler for X {\n    async fn list_tools(&self) {}\n}\n' > "$d/platform/services/other/src/server.rs"
  out=$(scan "$d")
  [[ "$out" == *"hand-written tool: platform/services/other/src/server.rs"* ]] || return 1
}

@test "NEGATIVE PROOF: a missing runner fails loud, never passes vacuously" {
  d="$BATS_TEST_TMPDIR/gone"; clean_fixture "$d"
  rm "$d/$RUNNER_SERVER"
  out=$(scan "$d")
  [[ "$out" == *"MISSING"* ]] || return 1
}

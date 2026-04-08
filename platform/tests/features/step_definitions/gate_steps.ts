import { Given, When, Then, Before, After } from '@cucumber/cucumber';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import assert from 'assert';

const HOOK_SHIM = '/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim';
const STATE_DIR = '/tmp/claude-team-scan';
const BRIEFS_DIR = '/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/wren/briefs';
const HOME = process.env.HOME || '/Users/jeffbridwell';
const TEST_CARD_ID = '99998';

// Test isolation — each scenario gets a unique session and clean state
const ALL_ROLES = ['kade', 'silas', 'wren'];

interface TestContext {
  role: string;
  cardType: string;
  sessionId: string;
  sessionLines: string[];
  hookResult: { stdout: string; stderr: string; exitCode: number } | null;
  stateBackups: Map<string, string | null>;
  targetFile: string;
  targetTool: string;
  cwd: string;
}

let ctx: TestContext;

Before(function () {
  ctx = {
    role: 'kade',
    cardType: 'new',
    sessionId: `bdd-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    sessionLines: [],
    hookResult: null,
    stateBackups: new Map(),
    targetFile: '/Users/jeffbridwell/CascadeProjects/chorus/platform/services/smoke-test.rs',
    targetTool: 'Edit',
    cwd: '/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/kade',
  };

  // Backup AND clear ALL role state files for deterministic tests.
  // is_fix_card() scans all three roles — if any live role happens to be
  // building a fix card, it contaminates the test.
  fs.mkdirSync(STATE_DIR, { recursive: true });
  for (const role of ALL_ROLES) {
    const statePath = path.join(STATE_DIR, `${role}-declared.json`);
    if (fs.existsSync(statePath)) {
      ctx.stateBackups.set(role, fs.readFileSync(statePath, 'utf-8'));
      fs.unlinkSync(statePath);
    } else {
      ctx.stateBackups.set(role, null);
    }
  }
});

After(function () {
  // Restore ALL role state files
  for (const role of ALL_ROLES) {
    const statePath = path.join(STATE_DIR, `${role}-declared.json`);
    const backup = ctx.stateBackups.get(role);
    if (backup !== null && backup !== undefined) {
      fs.writeFileSync(statePath, backup);
    } else if (fs.existsSync(statePath)) {
      fs.unlinkSync(statePath);
    }
  }

  // Clean up session JSONL
  const projectKey = ctx.cwd.replace(/\//g, '-').replace(/^-/, '');
  const jsonlPath = path.join(HOME, '.claude', 'projects', `-${projectKey}`, `${ctx.sessionId}.jsonl`);
  if (fs.existsSync(jsonlPath)) {
    fs.unlinkSync(jsonlPath);
  }

  // Clean up any test demo briefs
  const demoBrief = path.join(BRIEFS_DIR, `2026-01-01-demo-${TEST_CARD_ID}.md`);
  if (fs.existsSync(demoBrief)) {
    fs.unlinkSync(demoBrief);
  }

  // Clean up test pair file
  const pairFile = `/tmp/pair-${TEST_CARD_ID}.md`;
  if (fs.existsSync(pairFile)) {
    fs.unlinkSync(pairFile);
  }
});

// --- State setup helpers ---

function writeCardState(role: string, cardType: string): void {
  fs.mkdirSync(STATE_DIR, { recursive: true });
  const statePath = path.join(STATE_DIR, `${role}-declared.json`);
  const state = {
    role,
    state: 'building',
    card: 99998,
    card_type: cardType,
    ts: Math.floor(Date.now() / 1000),
  };
  fs.writeFileSync(statePath, JSON.stringify(state));
}

function writeSessionJsonl(sessionId: string, cwd: string, lines: string[]): void {
  const projectKey = cwd.replace(/\//g, '-').replace(/^-/, '');
  const jsonlDir = path.join(HOME, '.claude', 'projects', `-${projectKey}`);
  fs.mkdirSync(jsonlDir, { recursive: true });
  const jsonlPath = path.join(jsonlDir, `${sessionId}.jsonl`);
  fs.writeFileSync(jsonlPath, lines.join('\n'));
}

function callHook(tool: string, toolInput: Record<string, string>, sessionId: string, cwd: string, role: string): { stdout: string; stderr: string; exitCode: number } {
  const input = JSON.stringify({
    tool_name: tool,
    tool_input: toolInput,
    session_id: sessionId,
    cwd,
    deploy_role: role,
  });

  try {
    const stdout = execSync(`echo '${input.replace(/'/g, "'\\''")}' | DEPLOY_ROLE=${role} "${HOOK_SHIM}" pre-tool-use`, {
      encoding: 'utf-8',
      timeout: 5000,
      env: { ...process.env, DEPLOY_ROLE: role },
    });
    return { stdout: stdout.trim(), stderr: '', exitCode: 0 };
  } catch (err: any) {
    return {
      stdout: err.stdout?.trim() || '',
      stderr: err.stderr?.trim() || '',
      exitCode: err.status || 1,
    };
  }
}

// --- Given steps ---

Given('the hook binary is available', function () {
  assert.ok(fs.existsSync(HOOK_SHIM), `Hook binary not found at ${HOOK_SHIM}`);
});

Given('a role is building a fix card', function () {
  ctx.cardType = 'fix';
  writeCardState(ctx.role, 'fix');
});

Given('a role is building a new card', function () {
  ctx.cardType = 'new';
  writeCardState(ctx.role, 'new');
});

Given('a role is building an enhance card', function () {
  ctx.cardType = 'enhance';
  writeCardState(ctx.role, 'enhance');
});

Given('a role is building a chore card', function () {
  ctx.cardType = 'chore';
  writeCardState(ctx.role, 'chore');
});

Given('a role is building a swat card', function () {
  ctx.cardType = 'swat';
  writeCardState(ctx.role, 'swat');
});

// --- Log evidence steps ---

Given('they have not read any log files', function () {
  // Append neutral content — no log markers. Don't overwrite existing lines.
  if (ctx.sessionLines.length === 0) {
    ctx.sessionLines.push('{"type":"assistant","content":"Starting work on the card."}');
  }
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have read {string}', function (logRef: string) {
  ctx.sessionLines.push(
    `{"type":"tool_use","name":"Bash","input":{"command":"tail -50 ${logRef}"}}`,
    `{"type":"tool_result","content":"[2026-04-01] some log output..."}`,
    '{"type":"assistant","content":"Checked the logs, found relevant entries."}',
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

// --- Chorus/memory search steps ---

Given('they have not searched Chorus or memory', function () {
  // Append neutral content — no search markers. Don't overwrite existing lines.
  if (ctx.sessionLines.length === 0) {
    ctx.sessionLines.push('{"type":"assistant","content":"Starting work on the card."}');
  }
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have searched {string}', function (searchCmd: string) {
  ctx.sessionLines.push(
    `{"type":"tool_use","name":"Bash","input":{"command":"bash ${searchCmd}"}}`,
    '{"type":"tool_result","content":"Found 20 results: ..."}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have not produced a context synthesis', function () {
  // Append a non-synthesis assistant message
  ctx.sessionLines.push('{"type":"assistant","content":"Let me look at this."}');
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have produced a synthesis with {string}', function (synthesisText: string) {
  ctx.sessionLines.push(`{"type":"assistant","content":"${synthesisText}"}`);
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have not run git log on the target file', function () {
  // No git log in session — already the default from search steps
});

Given('they have git history for the target file', function () {
  // Add git log for the target file (gate-check.ts from cross-domain step)
  ctx.sessionLines.push(
    '{"type":"tool_use","name":"Bash","input":{"command":"git log --oneline gate-check.ts"}}',
    '{"type":"tool_result","content":"abc1234 last change to gate-check.ts"}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have not stated what the logs revealed', function () {
  // Log was read (from a previous Given step) but no "Log evidence:" synthesis
  // Just ensure no log synthesis markers in session — don't add any
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have stated log findings without reading logs', function () {
  // Has synthesis but no log read — fabricated findings
  ctx.sessionLines.push(
    '{"type":"assistant","content":"Log evidence: the seed pipeline is dropping messages at the routing step."}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have stated {string}', function (statement: string) {
  ctx.sessionLines.push(
    `{"type":"assistant","content":"${statement}"}`
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have context synthesis but no log evidence', function () {
  // Satisfy memory_gate (search + synthesis + git log) but NOT log_first (no log markers).
  // This isolates log_first_gate as the only gate that should block.
  ctx.sessionLines = [
    '{"type":"tool_use","name":"Bash","input":{"command":"bash chorus-query.sh search gate fix"}}',
    '{"type":"tool_result","content":"Found 10 results: prior gate fixes..."}',
    '{"type":"assistant","content":"Prior work: checked gate history. Current state: gate needs fix. Approach: apply targeted fix."}',
    '{"type":"tool_use","name":"Bash","input":{"command":"git log --oneline app.ts"}}',
    '{"type":"tool_result","content":"abc1234 last change to app.ts"}',
  ];
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have full context synthesis for a fix', function () {
  // Satisfy memory_gate (search + synthesis + git log) AND log_first (log read + log synthesis)
  ctx.sessionLines.push(
    '{"type":"tool_use","name":"Bash","input":{"command":"bash chorus-query.sh search gate fix"}}',
    '{"type":"tool_result","content":"Found 10 results: prior gate fixes..."}',
    '{"type":"assistant","content":"Prior work: checked gate history. Current state: gate needs fix. Approach: apply targeted fix based on log evidence. Log evidence: chorus.log shows gate smoke check failing at session boot — the deny response is empty."}',
    '{"type":"tool_use","name":"Bash","input":{"command":"git log --oneline app.ts"}}',
    '{"type":"tool_result","content":"abc1234 last change to app.ts"}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have full context synthesis without log evidence', function () {
  // Satisfy memory_gate (search + synthesis + git log) but NOT log_first (no "Log evidence:" marker)
  ctx.sessionLines.push(
    '{"type":"tool_use","name":"Bash","input":{"command":"bash chorus-query.sh search gate fix"}}',
    '{"type":"tool_result","content":"Found 10 results: prior gate fixes..."}',
    '{"type":"assistant","content":"Prior work: checked gate history. Current state: gate needs fix. Approach: apply targeted fix."}',
    '{"type":"tool_use","name":"Bash","input":{"command":"git log --oneline app.ts"}}',
    '{"type":"tool_result","content":"abc1234 last change to app.ts"}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

// --- Demo brief steps ---

Given('a demo brief exists for the card', function () {
  const briefPath = path.join(BRIEFS_DIR, `2026-01-01-demo-${TEST_CARD_ID}.md`);
  fs.writeFileSync(briefPath, `# Demo ready: #${TEST_CARD_ID}\nTest demo brief for BDD gate specs.\n`);
});

Given('no demo brief exists for the card', function () {
  // Ensure no demo brief exists for the test card
  const briefPath = path.join(BRIEFS_DIR, `2026-01-01-demo-${TEST_CARD_ID}.md`);
  if (fs.existsSync(briefPath)) {
    fs.unlinkSync(briefPath);
  }
});

Given('the card is owned by the building role', function () {
  // Card owner matches the role — accept gate should block self-accept
  // The accept gate fetches card view from board-ts, so we can't easily fake ownership.
  // Instead, we rely on the fact that demo_gate runs first and blocks without demo.
  // For self-accept tests, the accept_gate reads card owner from board CLI.
  // Since we use a fake card ID (99998), the board won't find it and owner will be empty.
  // Empty owner != role, so self-accept won't fire. We need a different approach.
  // For now, this step is a no-op — the accept gate's self-accept check depends on
  // the board having the card. We test the demo gate path instead.
});

Given('the card is owned by a different role', function () {
  // Card owner differs from building role — accept gate allows
});

Given('the card is a strategy card owned by the building role', function () {
  // Strategy cards (chunk:strategy) are exempt from self-accept block
});

// --- TDD evidence steps ---

Given('they have not run any tests in the session', function () {
  if (ctx.sessionLines.length === 0) {
    ctx.sessionLines.push('{"type":"assistant","content":"Starting work on the card."}');
  }
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have run {string} in the session', function (testCmd: string) {
  ctx.sessionLines.push(
    `{"type":"tool_use","name":"Bash","input":{"command":"${testCmd}"}}`,
    '{"type":"tool_result","content":"test result: ok. 10 passed; 0 failed"}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have not edited any test files', function () {
  // Session has no test file edits — only neutral content
  if (ctx.sessionLines.length === 0) {
    ctx.sessionLines.push('{"type":"assistant","content":"Starting work on the card."}');
  }
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have edited a test file', function () {
  // Session shows a test file was edited before production code
  ctx.sessionLines.push(
    '{"type":"tool_use","name":"Edit","input":{"file_path":"/Users/jeffbridwell/CascadeProjects/chorus/platform/tests/features/gates/tdd.feature","old_string":"x","new_string":"y"}}',
    '{"type":"tool_result","content":"File updated successfully"}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

When('they try to edit a test file', function () {
  ctx.targetTool = 'Edit';
  ctx.targetFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/tests/features/gates/tdd.feature';
  ctx.hookResult = callHook('Edit', {
    file_path: ctx.targetFile,
    old_string: 'x',
    new_string: 'y',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

// --- Pair session steps ---

Given('a pair session is active', function () {
  // Create a recent pair file
  fs.writeFileSync('/tmp/pair-99998.md', '# Pair: #99998\nTest pair session\n');
});

Given('no pair session is active', function () {
  // Remove any test pair files — but be careful not to remove real pair sessions.
  // Only remove our test card's pair file.
  const pairFile = '/tmp/pair-99998.md';
  if (fs.existsSync(pairFile)) {
    fs.unlinkSync(pairFile);
  }
});

// --- Stop-on-error steps ---

Given('the previous tool was {string} with exit code {int}', function (cmd: string, exitCode: number) {
  // stop_on_error is a PostToolUse hook that checks the previous tool's response.
  // For PreToolUse testing, we can't easily simulate PostToolUse state.
  // These scenarios test the benign-command exemption list.
  ctx.sessionLines.push(
    `{"type":"tool_use","name":"Bash","input":{"command":"${cmd}"}}`,
    `{"type":"tool_result","content":"Exit code ${exitCode}"}`
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

// --- When steps ---

When('they try to edit a code file', function () {
  ctx.targetTool = 'Edit';
  ctx.targetFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/services/smoke-test.rs';
  ctx.hookResult = callHook('Edit', {
    file_path: ctx.targetFile,
    old_string: 'x',
    new_string: 'y',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to edit a code file in their own domain', function () {
  ctx.targetTool = 'Edit';
  ctx.targetFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/kade/src/app.ts';
  ctx.hookResult = callHook('Edit', {
    file_path: ctx.targetFile,
    old_string: 'x',
    new_string: 'y',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to edit a cross-domain code file', function () {
  ctx.targetTool = 'Edit';
  // Cross-domain for kade (engineer): must NOT contain /engineer/ or /src/
  // (memory_gate treats both as Kade's own domain)
  ctx.targetFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/wren/scripts/gate-check.ts';
  ctx.hookResult = callHook('Edit', {
    file_path: ctx.targetFile,
    old_string: 'x',
    new_string: 'y',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to edit a file in their own domain', function () {
  ctx.targetTool = 'Edit';
  ctx.targetFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/roles/kade/src/app.ts';
  ctx.hookResult = callHook('Edit', {
    file_path: ctx.targetFile,
    old_string: 'x',
    new_string: 'y',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to edit a markdown file', function () {
  ctx.targetTool = 'Edit';
  ctx.targetFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/README.md';
  ctx.hookResult = callHook('Edit', {
    file_path: ctx.targetFile,
    old_string: 'x',
    new_string: 'y',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to mark the card done', function () {
  // demo_gate detects "cards done" or "board-ts done" in the command
  ctx.hookResult = callHook('Bash', {
    command: `cards done ${TEST_CARD_ID}`,
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to run demo on the card', function () {
  ctx.hookResult = callHook('Skill', {
    skill: 'demo',
    args: TEST_CARD_ID,
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to run acp on the card', function () {
  ctx.hookResult = callHook('Skill', {
    skill: 'acp',
    args: TEST_CARD_ID,
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they try to read a code file', function () {
  ctx.targetTool = 'Read';
  ctx.targetFile = '/Users/jeffbridwell/CascadeProjects/chorus/platform/services/smoke-test.rs';
  ctx.hookResult = callHook('Read', {
    file_path: ctx.targetFile,
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

// --- Memory-first gate steps (#1951) ---

Given('they have not queried memory endpoints', function () {
  if (ctx.sessionLines.length === 0) {
    ctx.sessionLines.push('{"type":"assistant","content":"Starting work on the card."}');
  }
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have queried card-story endpoint', function () {
  ctx.sessionLines.push(
    '{"type":"tool_use","name":"Bash","input":{"command":"curl -s http://localhost:3340/api/chorus/card-story/1951"}}',
    '{"type":"tool_result","content":"{ \\"card\\": 1951, \\"timeline\\": [] }"}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

Given('they have run a chorus search', function () {
  ctx.sessionLines.push(
    '{"type":"tool_use","name":"Bash","input":{"command":"bash ~/.chorus/scripts/chorus-query.sh search \\"alert nudge\\""}}',
    '{"type":"tool_result","content":"Found 20 results..."}'
  );
  writeSessionJsonl(ctx.sessionId, ctx.cwd, ctx.sessionLines);
});

When('they grep for card context {string}', function (pattern: string) {
  ctx.hookResult = callHook('Grep', {
    pattern,
    path: '',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they grep in a session context path {string}', function (contextPath: string) {
  ctx.hookResult = callHook('Grep', {
    pattern: 'bdd-no-match-a1b2c3d4e5f6',
    path: contextPath,
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they grep for a code pattern {string}', function (pattern: string) {
  ctx.hookResult = callHook('Grep', {
    pattern,
    path: '',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they grep for a code pattern in {string}', function (codePath: string) {
  ctx.hookResult = callHook('Grep', {
    pattern: 'bdd-no-match-a1b2c3d4e5f6',
    path: codePath,
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they bash grep session context {string}', function (command: string) {
  ctx.hookResult = callHook('Bash', {
    command,
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

Then('the gate allows the search', function () {
  assert.ok(ctx.hookResult, 'Hook was not called');
  const hasDeny = ctx.hookResult.stdout.includes('"deny"');
  const hasBlock = ctx.hookResult.exitCode === 2;
  assert.ok(
    !hasDeny && !hasBlock,
    `Expected gate to allow search but got:\nstdout: ${ctx.hookResult.stdout}\nstderr: ${ctx.hookResult.stderr}\nexit: ${ctx.hookResult.exitCode}`
  );
});

// --- Compound search steps (#2004) ---

let lastGrepSession = '';

When('they grep for a term with Chorus results {string}', function (pattern: string) {
  lastGrepSession = ctx.sessionId;
  ctx.hookResult = callHook('Grep', {
    pattern,
    path: '',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they grep for a term with no Chorus results {string}', function (pattern: string) {
  ctx.hookResult = callHook('Grep', {
    pattern,
    path: '',
  }, ctx.sessionId, ctx.cwd, ctx.role);
});

When('they retry the same grep {string}', function (pattern: string) {
  ctx.hookResult = callHook('Grep', {
    pattern,
    path: '',
  }, lastGrepSession, ctx.cwd, ctx.role);
});

Then('the deny message contains {string}', function (expectedFragment: string) {
  assert.ok(ctx.hookResult, 'Hook was not called');
  const output = ctx.hookResult.stdout + ctx.hookResult.stderr;
  assert.ok(
    output.includes(expectedFragment),
    `Expected deny message to contain "${expectedFragment}" but got:\n${output.slice(0, 500)}`
  );
});

// --- Then steps ---

Then('the gate blocks with {string}', function (expectedFragment: string) {
  assert.ok(ctx.hookResult, 'Hook was not called');
  const output = ctx.hookResult.stdout + ctx.hookResult.stderr;
  assert.ok(
    output.toLowerCase().includes(expectedFragment.toLowerCase()),
    `Expected gate to block with "${expectedFragment}" but got:\nstdout: ${ctx.hookResult.stdout}\nstderr: ${ctx.hookResult.stderr}`
  );
});

Then('the gate allows the edit', function () {
  assert.ok(ctx.hookResult, 'Hook was not called');
  // Allow = no deny message in stdout, and no blocking stderr
  const hasDeny = ctx.hookResult.stdout.includes('"deny"');
  const hasBlock = ctx.hookResult.stderr.includes('gate') && ctx.hookResult.exitCode !== 0;
  assert.ok(
    !hasDeny && !hasBlock,
    `Expected gate to allow but got:\nstdout: ${ctx.hookResult.stdout}\nstderr: ${ctx.hookResult.stderr}\nexit: ${ctx.hookResult.exitCode}`
  );
});

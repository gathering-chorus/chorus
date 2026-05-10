// #2880: BDD tests for /build + /deploy substrate — exercise actual scripts.
//
// Same convention as #2875 (Wren's demo steps): step defs invoke real
// shell/CLI surfaces (chorus-log, chorus-bin-install, chorus-build,
// git-queue.sh helper, building-pipeline-health) and assert against
// observable outputs (chorus.log lines, filesystem, exit codes).
//
// What's testable in this style:
//   - chorus-log numeric-key coercion (#2876): emit known fields, parse JSON
//   - chorus-bin-install atomic move + binary.deployed event (#2734): fixture binary
//   - chorus-build canonical-sync invariant abort (#2863): non-git tree → exit non-zero
//   - git-queue export_card_id_from_branch (#2876): fixture branch → env
//
// What's tagged @wip @gap-2881:
//   - building-pipeline-health direct-read mode — script queries Loki only
//     today; testing pairing logic without index lag requires a direct-read
//     mode. Filed as #2881 follow-on; scenarios fail loud until that lands.

import { Given, When, Then, Before, After, setDefaultTimeout } from '@cucumber/cucumber';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import assert from 'assert';

setDefaultTimeout(30_000);

const CHORUS_ROOT = '/Users/jeffbridwell/CascadeProjects/chorus';
const CHORUS_LOG_BIN = `${CHORUS_ROOT}/platform/scripts/chorus-log`;
const CHORUS_LOG_FILE = process.env.CHORUS_LOG_FILE || `${process.env.HOME}/.chorus/chorus.log`;
const BIN_INSTALL = `${CHORUS_ROOT}/platform/scripts/chorus-bin-install`;
const CHORUS_BUILD = `${CHORUS_ROOT}/platform/scripts/chorus-build`;
const GIT_QUEUE = `${CHORUS_ROOT}/platform/scripts/git-queue.sh`;
const CHORUS_BIN_DIR = `${process.env.HOME}/.chorus/bin`;

interface Ctx {
  traceMarker: string;
  logSnapshotBytes: number;
  fixtureBinaryPath: string | null;
  fixtureBinaryName: string | null;
  tempGitRepo: string | null;
  tempNonGitDir: string | null;
  lastStdout: string;
  lastStderr: string;
  lastExitCode: number;
  exportedCardId: string | null;
}

let ctx: Ctx;

Before({ tags: '@build-deploy-skill' }, function () {
  const stat = fs.existsSync(CHORUS_LOG_FILE) ? fs.statSync(CHORUS_LOG_FILE) : null;
  ctx = {
    traceMarker: `bdd-build-deploy-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    logSnapshotBytes: stat ? stat.size : 0,
    fixtureBinaryPath: null,
    fixtureBinaryName: null,
    tempGitRepo: null,
    tempNonGitDir: null,
    lastStdout: '',
    lastStderr: '',
    lastExitCode: 0,
    exportedCardId: null,
  };
});

After({ tags: '@build-deploy-skill' }, function () {
  if (ctx?.fixtureBinaryPath && fs.existsSync(ctx.fixtureBinaryPath)) {
    try { fs.unlinkSync(ctx.fixtureBinaryPath); } catch { /* ignore */ }
  }
  if (ctx?.fixtureBinaryName) {
    const installed = path.join(CHORUS_BIN_DIR, ctx.fixtureBinaryName);
    if (fs.existsSync(installed)) {
      try { fs.unlinkSync(installed); } catch { /* ignore */ }
    }
  }
  if (ctx?.tempGitRepo && fs.existsSync(ctx.tempGitRepo)) {
    try { execSync(`rm -rf "${ctx.tempGitRepo}"`); } catch { /* ignore */ }
  }
  if (ctx?.tempNonGitDir && fs.existsSync(ctx.tempNonGitDir)) {
    try { execSync(`rm -rf "${ctx.tempNonGitDir}"`); } catch { /* ignore */ }
  }
});

// --- Helpers ---

function readLogSinceSnapshot(): string {
  if (!fs.existsSync(CHORUS_LOG_FILE)) return '';
  const fd = fs.openSync(CHORUS_LOG_FILE, 'r');
  try {
    const stat = fs.fstatSync(fd);
    const len = stat.size - ctx.logSnapshotBytes;
    if (len <= 0) return '';
    const buf = Buffer.alloc(len);
    fs.readSync(fd, buf, 0, len, ctx.logSnapshotBytes);
    return buf.toString('utf-8');
  } finally {
    fs.closeSync(fd);
  }
}

function waitForEventInLog(eventName: string, timeoutMs = 5000): Record<string, unknown> | null {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const tail = readLogSinceSnapshot();
    for (const line of tail.split('\n').filter(l => l.includes(`"event":"${eventName}"`))) {
      if (line.includes(ctx.traceMarker)) {
        try {
          return JSON.parse(line);
        } catch {
          // Skip malformed lines (parallel writes can interleave).
        }
      }
    }
    execSync('sleep 0.1');
  }
  return null;
}

// --- Background ---
// Note: "the chorus.log spine is writable" reused from demo_steps.ts; defining
// it here too would cause cucumber ambiguous-step refusal.

Given('the chorus bin install dir exists', function () {
  assert.ok(fs.existsSync(CHORUS_BIN_DIR), `bin dir missing at ${CHORUS_BIN_DIR}`);
});

// --- Scenario 1: chorus-log numeric-key coercion ---

Given('a unique trace marker for this run', function () {
  // Already set in Before; this just makes the Gherkin readable.
  assert.ok(ctx.traceMarker.length > 0);
});

When('chorus-log emits build.push.completed with card_id={int} and exit_code={int}', function (cardId: number, exitCode: number) {
  execSync(`${CHORUS_LOG_BIN} build.push.completed silas card_id=${cardId} exit_code=${exitCode} trace_marker=${ctx.traceMarker}`,
    { encoding: 'utf-8' });
});

When('chorus-log emits build.queue.acquired with title={string} and card_id={int}', function (title: string, cardId: number) {
  execSync(`${CHORUS_LOG_BIN} build.queue.acquired silas title=${title} card_id=${cardId} trace_marker=${ctx.traceMarker}`,
    { encoding: 'utf-8' });
});

When('chorus-log emits build.queue.acquired with title={int} and card_id={int}', function (title: number, cardId: number) {
  // Cucumber parses bare numbers as int even in argument position; this overload
  // covers the all-digits title case explicitly.
  execSync(`${CHORUS_LOG_BIN} build.queue.acquired silas title=${title} card_id=${cardId} trace_marker=${ctx.traceMarker}`,
    { encoding: 'utf-8' });
});

Then('the build-deploy event lands in chorus.log within {int} seconds', function (secs: number) {
  // The scenario specifies which event — re-derive from the last command by
  // scanning recent emits. Simpler: look for any event with our trace marker.
  const tail = readLogSinceSnapshot();
  const deadline = Date.now() + secs * 1000;
  let found = false;
  while (Date.now() < deadline) {
    if (readLogSinceSnapshot().includes(ctx.traceMarker)) {
      found = true;
      break;
    }
    execSync('sleep 0.1');
  }
  assert.ok(found, `no event with trace marker ${ctx.traceMarker} in chorus.log tail (${tail.length} bytes)`);
});

Then('the event JSON has card_id as an integer not a string', function () {
  const tail = readLogSinceSnapshot();
  const line = tail.split('\n').reverse().find(l => l.includes(ctx.traceMarker));
  assert.ok(line, 'no matching log line found');
  // Substring check: `"card_id":NNN,` (integer) vs `"card_id":"NNN",` (string).
  assert.ok(/"card_id":\d+[,}]/.test(line!), `card_id not in unquoted integer form: ${line}`);
  assert.ok(!/"card_id":"\d+"/.test(line!), `card_id is string-quoted: ${line}`);
});

Then('the event JSON has exit_code as an integer not a string', function () {
  const tail = readLogSinceSnapshot();
  const line = tail.split('\n').reverse().find(l => l.includes(ctx.traceMarker));
  assert.ok(line, 'no matching log line found');
  assert.ok(/"exit_code":\d+[,}]/.test(line!), `exit_code not in unquoted integer form: ${line}`);
  assert.ok(!/"exit_code":"\d+"/.test(line!), `exit_code is string-quoted: ${line}`);
});

Then('the event JSON has card_id as an integer', function () {
  const tail = readLogSinceSnapshot();
  const line = tail.split('\n').reverse().find(l => l.includes(ctx.traceMarker));
  assert.ok(line, 'no matching log line found');
  assert.ok(/"card_id":\d+[,}]/.test(line!), `card_id not in unquoted integer form: ${line}`);
});

Then('the event JSON has title as a string with value {string}', function (expected: string) {
  const tail = readLogSinceSnapshot();
  const line = tail.split('\n').reverse().find(l => l.includes(ctx.traceMarker));
  assert.ok(line, 'no matching log line found');
  const expectedFragment = `"title":"${expected}"`;
  assert.ok(line!.includes(expectedFragment),
    `title not string-quoted with value "${expected}": ${line}`);
});

// --- Scenario 2: chorus-bin-install ---

Given('a fixture binary exists at a temp path', function () {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'bdd-bin-'));
  const binaryPath = path.join(tmp, `fixture-${ctx.traceMarker}`);
  // A real Mach-O binary is required for codesign --display, but
  // chorus-bin-install treats codesign failure as best-effort (cdhash=unknown
  // when codesign can't read it). A script suffices as a fixture.
  fs.writeFileSync(binaryPath, '#!/bin/sh\necho fixture-binary\n');
  fs.chmodSync(binaryPath, 0o755);
  ctx.fixtureBinaryPath = binaryPath;
  ctx.fixtureBinaryName = `fixture-${ctx.traceMarker}`;
});

When('chorus-bin-install installs the fixture binary under a fixture name', function () {
  assert.ok(ctx.fixtureBinaryPath && ctx.fixtureBinaryName);
  try {
    const out = execSync(`${BIN_INSTALL} "${ctx.fixtureBinaryPath}" "${ctx.fixtureBinaryName}"`,
      { encoding: 'utf-8' });
    ctx.lastStdout = out;
    ctx.lastExitCode = 0;
  } catch (e: unknown) {
    const err = e as { status?: number; stdout?: Buffer; stderr?: Buffer };
    ctx.lastExitCode = err.status ?? 1;
    ctx.lastStdout = err.stdout?.toString() ?? '';
    ctx.lastStderr = err.stderr?.toString() ?? '';
  }
});

Then('the binary lands in the chorus bin install dir under the fixture name', function () {
  assert.equal(ctx.lastExitCode, 0, `install failed: ${ctx.lastStderr}`);
  assert.ok(ctx.fixtureBinaryName);
  const dest = path.join(CHORUS_BIN_DIR, ctx.fixtureBinaryName!);
  assert.ok(fs.existsSync(dest), `binary not at ${dest}`);
  assert.ok(fs.statSync(dest).mode & 0o100, 'binary not executable');
});

Then('a binary.deployed spine event fires with the fixture name', function () {
  const ev = waitForEventInLog('binary.deployed', 5000);
  // chorus-bin-install emits via chorus-log when available; check whichever
  // form is present (envelope name binary present or in payload).
  const tail = readLogSinceSnapshot();
  const line = tail.split('\n').reverse().find(l =>
    l.includes('"event":"binary.deployed"') && l.includes(ctx.fixtureBinaryName!)
  );
  assert.ok(line, `binary.deployed event with name=${ctx.fixtureBinaryName} not in log tail (event=${ev ? 'yes' : 'no'})`);
});

// --- Scenario 3: chorus-build canonical-sync invariant ---

Given('a temp dir that is not a git repo', function () {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'bdd-non-git-'));
  ctx.tempNonGitDir = tmp;
});

When('chorus-build chorus-hooks runs against the temp dir as CHORUS_HOME', function () {
  assert.ok(ctx.tempNonGitDir);
  try {
    const out = execSync(`CHORUS_ROOT="${ctx.tempNonGitDir}" "${CHORUS_BUILD}" chorus-hooks 2>&1`,
      { encoding: 'utf-8' });
    ctx.lastStdout = out;
    ctx.lastExitCode = 0;
  } catch (e: unknown) {
    const err = e as { status?: number; stdout?: Buffer; stderr?: Buffer };
    ctx.lastExitCode = err.status ?? 1;
    ctx.lastStdout = (err.stdout?.toString() ?? '') + (err.stderr?.toString() ?? '');
    ctx.lastStderr = err.stderr?.toString() ?? '';
  }
});

Then('the script exits non-zero', function () {
  assert.notEqual(ctx.lastExitCode, 0, `script should have failed, exit=${ctx.lastExitCode}`);
});

Then('the stderr contains {string} and mentions canonical-sync', function (needle: string) {
  const combined = ctx.lastStdout + ctx.lastStderr;
  assert.ok(combined.includes(needle), `${needle} not in output: ${combined.slice(0, 500)}`);
  assert.ok(/canonical|origin\/main|fast-forward/i.test(combined),
    `output doesn't mention canonical-sync: ${combined.slice(0, 500)}`);
});

// --- Scenario 4: building-pipeline-health pairing (tagged @wip @gap-2881) ---

Given('a synthesized chorus_acp.completed event for card {int}', function (_cardId: number) {
  return 'pending'; // @gap-2881: needs CHORUS_LOG_DIRECT mode in building-pipeline-health
});

Given('a matching chorus_acp.release-trigger.completed event for card {int}', function (_cardId: number) {
  return 'pending';
});

Given('a matching deploy.completed event for card {int}', function (_cardId: number) {
  return 'pending';
});

Given('no release-trigger event for card {int}', function (_cardId: number) {
  return 'pending';
});

When('building-pipeline-health runs in direct-read mode against the trace marker', function () {
  return 'pending';
});

Then('the script exits 0', function () {
  return 'pending';
});

Then('the script exits 1', function () {
  return 'pending';
});

Then('the JSON output has unpaired_release_trigger=0 and unpaired_pipeline_run=0', function () {
  return 'pending';
});

Then('the JSON output has unpaired_release_trigger>={int}', function (_n: number) {
  return 'pending';
});

Then('the affected card {int} appears in the verbose output', function (_cardId: number) {
  return 'pending';
});

// --- Scenario 5: git-queue export_card_id_from_branch ---

Given('a temp git repo on branch {word}', function (branch: string) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'bdd-git-'));
  execSync(`cd "${tmp}" && git init --quiet --initial-branch=main && git config user.email test@test && git config user.name test && git commit --allow-empty -m init --quiet`);
  if (branch !== 'main') {
    execSync(`cd "${tmp}" && git checkout -b "${branch}" --quiet`);
  }
  ctx.tempGitRepo = tmp;
});

When('the export_card_id_from_branch helper runs', function () {
  assert.ok(ctx.tempGitRepo);
  // Extract just the function from git-queue.sh and invoke it in a subshell
  // with REPO_ROOT set to the fixture. Same pattern as the bats contract pin.
  const cmd = `
    REPO_ROOT="${ctx.tempGitRepo}"
    eval "$(sed -n '/^export_card_id_from_branch()/,/^}/p' "${GIT_QUEUE}")"
    export_card_id_from_branch
    echo "CARD_ID=\${CHORUS_CARD_ID:-UNSET}"
  `;
  const out = execSync(`bash -c '${cmd.replace(/'/g, "'\\''")}'`, { encoding: 'utf-8' });
  const match = out.match(/CARD_ID=(.+)/);
  ctx.exportedCardId = match ? match[1].trim() : null;
});

Then('CHORUS_CARD_ID equals {string}', function (expected: string) {
  assert.equal(ctx.exportedCardId, expected,
    `expected CHORUS_CARD_ID="${expected}", got "${ctx.exportedCardId}"`);
});

Then('CHORUS_CARD_ID is unset', function () {
  assert.equal(ctx.exportedCardId, 'UNSET',
    `expected CHORUS_CARD_ID unset, got "${ctx.exportedCardId}"`);
});

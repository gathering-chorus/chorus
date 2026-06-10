// #3329 — BEHAVIOR tests for the executeWerkVerb wrapper layer (#3324 audit, matrix
// gap 2). Before this file the wrapper had surface pins only: deleting #3320's
// CHORUS_INVOKER (the transport-drop fix), breaking the reason= refusal parse, or
// un-registering a verb tool all kept the suite green. These tests drive the REAL
// production path: real buildMcpServer, real executeWerkVerb spawn — only the verb
// binary is a stub, injected via the CHORUS_BIN env seam the wrapper already
// resolves binaries through (no production changes, no parallel parser).

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { tmpdir } from 'node:os';
import { mkdtempSync, writeFileSync, chmodSync } from 'node:fs';
import { join } from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer } from '../src/server';

// Stub verb binary: dumps argv + the wrapper-injected env as JSON on stdout, or
// fails with scripted stderr/exit to drive the refusal-parse paths.
function stubBin(dir: string, verb: string, body: string) {
  const p = join(dir, verb);
  writeFileSync(p, `#!/bin/sh\n${body}\n`);
  chmodSync(p, 0o755);
}

const DUMP_ENV = `printf '{"argv":"%s","DEPLOY_ROLE":"%s","CHORUS_ROLE":"%s","CHORUS_HOME":"%s","CHORUS_WERK_BASE":"%s","CHORUS_INVOKER":"%s"}' "$*" "$DEPLOY_ROLE" "$CHORUS_ROLE" "$CHORUS_HOME" "$CHORUS_WERK_BASE" "$CHORUS_INVOKER"`;

async function withServer(binDir: string, fn: (client: Client) => Promise<void>) {
  const orig = process.env.CHORUS_BIN;
  // Synthetic failures must NEVER reach the live error surface (the
  // feedback_no_live_role_identifiers_in_tests class — this suite's first run
  // broadcast "something exploded" as real mcp.error nudges onto Jeff's
  // terminal). Same containment dispatch-error-integration.test.ts uses:
  // chorus.log to a temp file, pulse/nudge notify to a dead port.
  const origLog = process.env.CHORUS_LOG_FILE;
  const origPulse = process.env.CHORUS_PULSE_URL;
  process.env.CHORUS_LOG_FILE = join(mkdtempSync(join(tmpdir(), 'wwc-log-')), 'chorus.log');
  process.env.CHORUS_PULSE_URL = 'http://127.0.0.1:1';
  // CHORUS_SYNTHETIC=1 — Silas's emitter-guard key (Wren is landing the
  // suppression): synthetic test traffic is marked at the source so the nudge
  // broadcast drops it while the event itself still logs.
  const origSynth = process.env.CHORUS_SYNTHETIC;
  process.env.CHORUS_SYNTHETIC = '1';
  process.env.CHORUS_BIN = binDir;
  const server = buildMcpServer(() => 'kade', { cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'werk-wrapper-contract-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    await fn(client);
  } finally {
    await client.close();
    await server.close();
    if (orig === undefined) delete process.env.CHORUS_BIN;
    else process.env.CHORUS_BIN = orig;
    if (origLog === undefined) delete process.env.CHORUS_LOG_FILE;
    else process.env.CHORUS_LOG_FILE = origLog;
    if (origPulse === undefined) delete process.env.CHORUS_PULSE_URL;
    else process.env.CHORUS_PULSE_URL = origPulse;
    if (origSynth === undefined) delete process.env.CHORUS_SYNTHETIC;
    else process.env.CHORUS_SYNTHETIC = origSynth;
  }
}

// Pull the stub's env dump back out of the wrapper's {ok, …, stdout} envelope.
function envDumpFrom(result: unknown): Record<string, string> {
  const content = (result as { content: Array<{ type: string; text: string }> }).content;
  const envelope = JSON.parse(content[0].text) as { ok: boolean; stdout: string };
  assert.equal(envelope.ok, true, `wrapper envelope ok: ${content[0].text}`);
  return JSON.parse(envelope.stdout) as Record<string, string>;
}

// ── AC1: the spawn-env contract, including #3320's CHORUS_INVOKER ──────────────
// Removing ANY of these from executeWerkVerb goes red here. CHORUS_INVOKER is the
// whole #3320 fix: without it werk-deploy can't detect the chorus-mcp self-deploy
// and the inline kickstart kills the caller (the transport-drop class, live 06-10).
test('werk verb spawn env carries the full contract incl. CHORUS_INVOKER=chorus-mcp', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'wwc-env-'));
  stubBin(dir, 'werk-commit', DUMP_ENV);
  await withServer(dir, async (client) => {
    const res = await client.callTool({
      name: 'werk-commit',
      arguments: { role: 'silas', card_id: 9001 },
    });
    const env = envDumpFrom(res);
    assert.equal(env.CHORUS_INVOKER, 'chorus-mcp', '#3320 invoker attribution — the self-deploy detach depends on this');
    assert.equal(env.DEPLOY_ROLE, 'silas', 'DEPLOY_ROLE = the role argument');
    assert.equal(env.CHORUS_ROLE, 'silas', 'CHORUS_ROLE mirrors the role');
    assert.ok(env.CHORUS_HOME.length > 0, 'CHORUS_HOME injected');
    assert.ok(env.CHORUS_WERK_BASE.length > 0, 'CHORUS_WERK_BASE injected');
    assert.equal(env.argv, '9001 silas', 'argv is <card> <role>');
  });
});

// ── AC4: werk-accept attribution — accepter is the CALLER, not the builder ─────
// DEC-048: the extraEnv override DEPLOY_ROLE=getCallerRole() is what makes the
// accept run under the accepting identity. Swapping it to the builder role would
// silently let builders self-accept; this pins the distinction.
test('werk-accept spawns with DEPLOY_ROLE = caller identity, not the builder role', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'wwc-accept-'));
  stubBin(dir, 'werk-accept', DUMP_ENV);
  await withServer(dir, async (client) => {
    const res = await client.callTool({
      name: 'werk-accept',
      arguments: { role: 'silas', card_id: 9002 },
    });
    const env = envDumpFrom(res);
    assert.equal(env.DEPLOY_ROLE, 'kade', 'accepter = caller (getCallerRole), the DEC-048 authority');
    assert.equal(env.CHORUS_ROLE, 'silas', 'builder role still names the werk');
    assert.equal(env.argv, '9002 silas', 'argv carries card + builder role');
  });
});

// ── AC2: reason= refusal parsing through the PRODUCTION parser ─────────────────
// error-capture.test.ts re-implemented this regex inline (passes-by-definition,
// flagged by the #3324 audit). These drive the real executeWerkVerb error path.
// A wrapper failure propagates as a thrown McpError — capture and assert on it.
async function callExpectingError(
  client: Client,
  name: string,
  args: Record<string, unknown>,
): Promise<string> {
  try {
    await client.callTool({ name, arguments: args });
  } catch (e) {
    return (e as Error).message;
  }
  assert.fail(`${name} was expected to fail and did not`);
}

test('non-zero exit with reason= marker surfaces the typed refusal', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'wwc-reason-'));
  stubBin(dir, 'werk-push', 'echo "werk-push: refused reason=branch-mismatch" >&2; exit 1');
  await withServer(dir, async (client) => {
    const text = await callExpectingError(client, 'werk-push', { role: 'kade', card_id: 9003 });
    assert.match(text, /werk-push-fail/, `verb named in the failure: ${text}`);
    assert.match(text, /reason=branch-mismatch/, `typed reason parsed from stderr: ${text}`);
    assert.match(text, /exit=1/, `exit code surfaced: ${text}`);
  });
});

test('reason marker is case-insensitive and accepts colon form', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'wwc-colon-'));
  stubBin(dir, 'werk-merge', 'echo "Reason: no-open-pr" >&2; exit 1');
  await withServer(dir, async (client) => {
    const text = await callExpectingError(client, 'werk-merge', { role: 'kade', card_id: 9004 });
    assert.match(text, /reason=no-open-pr/, `Reason: form parsed case-insensitively: ${text}`);
  });
});

test('exit 2 with no marker falls back to usage-error', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'wwc-usage-'));
  stubBin(dir, 'werk-pull', 'echo "usage: werk-pull <card> <role>" >&2; exit 2');
  await withServer(dir, async (client) => {
    const text = await callExpectingError(client, 'werk-pull', { role: 'kade', card_id: 9005 });
    assert.match(text, /reason=usage-error/, `exit-2 fallback: ${text}`);
  });
});

test('non-zero exit with no marker falls back to work-fail', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'wwc-workfail-'));
  stubBin(dir, 'werk-build', 'echo "something exploded" >&2; exit 1');
  await withServer(dir, async (client) => {
    const text = await callExpectingError(client, 'werk-build', { role: 'kade', card_id: 9006 });
    assert.match(text, /reason=work-fail/, `no-marker fallback: ${text}`);
  });
});

// ── AC3: the exact verb surface — additions AND removals both fail ─────────────
// The #3324 audit found only inclusion/absence spot-checks: un-registering
// werk-merge or resurrecting a #3311-deleted tool failed nothing. This pins the
// verb-shaped surface exactly. Changing it is a deliberate product decision that
// must touch this test in the same commit.
test('the verb-shaped MCP surface is exactly the eight verbs + chorus_werk + loom-gemba', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'wwc-surface-'));
  await withServer(dir, async (client) => {
    const names = (await client.listTools()).tools.map((t) => t.name);
    const verbShaped = names
      .filter((n) => n.startsWith('werk-') || n.startsWith('loom-') || n === 'chorus_werk')
      .sort();
    assert.deepEqual(
      verbShaped,
      [
        'chorus_werk',
        'loom-gemba',
        'werk-accept',
        'werk-build',
        'werk-commit',
        'werk-deploy',
        'werk-merge',
        'werk-pull',
        'werk-push',
        'werk-unpull', // #3299 — the /pull inverse joined the verb family (deliberate pin edit)
      ],
      'exact verb surface — a removal or resurrection must consciously edit this pin',
    );
    // The #3311 deletions stay dead, by name (belt over the exact-pin braces).
    for (const gone of ['chorus_werk_land', 'chorus_env_up', 'chorus_deploy', 'werk-finalize', 'chorus_acp']) {
      assert.ok(!names.includes(gone), `${gone} was deleted by #3311 and must not return silently`);
    }
  });
});

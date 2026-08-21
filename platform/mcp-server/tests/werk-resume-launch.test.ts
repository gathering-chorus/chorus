// @test-type: unit — hermetic: fixture git repos + tmp runsDir + in-memory MCP transport + captured spawn; no live act/werk
/**
 * #3956 — resume: a flake costs the flake, not the pipeline.
 *
 * The launcher opts a relaunch into resume (WERK_RESUME=1) ONLY when the prior
 * run FAILED and the werk tree (uncommitted edits included) is byte-identical
 * to the tree its legs passed against. The follower is the single writer of
 * per-leg proofs. Negative proofs: a changed tree or a proof-less pin must
 * never claim resume.
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { execFileSync } from 'child_process';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type SpawnFn } from '../src/server';
import { writeRun, readRun, recordLeg, currentWerkTreeHash } from '../src/werk-run-store';
import { parsePhaseTransitions } from '../src/werk-phase';

type SpawnCall = { command: string; args: string[]; opts: { env?: Record<string, string> } };
function captureSpawn(sink: SpawnCall[]): SpawnFn {
  return ((command: string, args: string[], opts: unknown) => {
    sink.push({ command, args, opts: opts as SpawnCall['opts'] });
    return { pid: 4242, unref() {} };
  }) as unknown as SpawnFn;
}

function mkWerk(base: string, card: number): string {
  const werk = path.join(base, `kade-${card}`);
  fs.mkdirSync(werk, { recursive: true });
  const git = (...a: string[]) => execFileSync('git', ['-C', werk, ...a]);
  git('init', '-q', '-b', `kade/${card}`);
  fs.writeFileSync(path.join(werk, 'f'), 'x\n');
  git('add', '.');
  git('-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '-m', `kade: #${card}`);
  return werk;
}

test('tree hash sees uncommitted edits and matches across calls', () => {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-'));
  const werk = mkWerk(base, 77);
  const h1 = currentWerkTreeHash(werk);
  assert.ok(h1, 'hash non-empty');
  assert.equal(currentWerkTreeHash(werk), h1, 'stable');
  fs.appendFileSync(path.join(werk, 'f'), 'dirty\n');
  assert.notEqual(currentWerkTreeHash(werk), h1, 'uncommitted edit changes the hash');
});

test('recordLeg appends onto the pin, last write per leg wins, no pin = no-op', () => {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'runs-'));
  recordLeg(88, 'build', 'pass', 'h1', runsDir); // no pin yet — must not throw
  writeRun({ runId: 'r1', card: 88, role: 'kade', go: false, phase: 'running', startedAt: 't' }, runsDir);
  recordLeg(88, 'build', 'pass', 'h1', runsDir);
  recordLeg(88, 'test', 'fail', 'h1', runsDir);
  recordLeg(88, 'test', 'pass', 'h1', runsDir); // rerun flips it
  const run = readRun(88, runsDir);
  assert.deepEqual(run?.legs, [
    { leg: 'build', verdict: 'pass', tree_hash: 'h1' },
    { leg: 'test', verdict: 'pass', tree_hash: 'h1' },
  ]);
});

test('follower parse yields the transitions leg-recording consumes', () => {
  const chunk = [
    '[werk/werk]   ✅  Success - Main build [10s]',
    '[werk/werk]   ✅  Success - Main merge [5s]',
    '[werk/werk]   ❌  Failure - Main test [90s]',
  ].join('\n');
  const legs = parsePhaseTransitions(chunk).map((e) => `${e.phase}:${e.state}`);
  assert.deepEqual(legs, ['build:pass', 'merge:pass', 'test:fail']);
});

async function launchWerk(
  runsDir: string, werkBase: string, sink: SpawnCall[], card: number,
): Promise<void> {
  const prev = process.env.CHORUS_WERK_BASE;
  process.env.CHORUS_WERK_BASE = werkBase;
  try {
    const server = buildMcpServer(() => 'kade', { spawnFn: captureSpawn(sink), runsDir, cardsPath: '/fake/cards' });
    const [ct, st] = InMemoryTransport.createLinkedPair();
    const client = new Client({ name: 'resume-test', version: '1.0' });
    await Promise.all([server.connect(st), client.connect(ct)]);
    try {
      await client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id: card } });
    } finally { await client.close(); await server.close(); }
  } finally {
    if (prev === undefined) delete process.env.CHORUS_WERK_BASE; else process.env.CHORUS_WERK_BASE = prev;
  }
}

test('relaunch of a FAILED run with unchanged tree sets WERK_RESUME=1 and carries legs', async () => {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'runs-'));
  const werkBase = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-'));
  const werk = mkWerk(werkBase, 91);
  const th = currentWerkTreeHash(werk);
  writeRun({
    runId: 'r-old', card: 91, role: 'kade', go: false, phase: 'failed', startedAt: 't',
    legs: [{ leg: 'build', verdict: 'pass', tree_hash: th }],
  }, runsDir);
  const sink: SpawnCall[] = [];
  await launchWerk(runsDir, werkBase, sink, 91);
  const act = sink.find((c) => c.command === 'bash');
  assert.equal(act?.opts.env?.WERK_RESUME, '1', 'resume flag set');
  assert.deepEqual(readRun(91, runsDir)?.legs,
    [{ leg: 'build', verdict: 'pass', tree_hash: th }], 'failed run legs carried onto the fresh pin');
});

test('NEGATIVE PROOF (#3734): tree changed since the failure — resume NOT claimed', async () => {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'runs-'));
  const werkBase = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-'));
  const werk = mkWerk(werkBase, 92);
  const th = currentWerkTreeHash(werk);
  writeRun({
    runId: 'r-old', card: 92, role: 'kade', go: false, phase: 'failed', startedAt: 't',
    legs: [{ leg: 'build', verdict: 'pass', tree_hash: th }],
  }, runsDir);
  fs.appendFileSync(path.join(werk, 'f'), 'drift\n');
  const sink: SpawnCall[] = [];
  await launchWerk(runsDir, werkBase, sink, 92);
  const act = sink.find((c) => c.command === 'bash');
  assert.equal(act?.opts.env?.WERK_RESUME, undefined, 'no resume on drift');
  assert.equal(readRun(92, runsDir)?.legs, undefined, 'stale legs NOT carried');
});

test('NEGATIVE PROOF (#3734): failed run with NO leg proofs — resume NOT claimed', async () => {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'runs-'));
  const werkBase = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-'));
  mkWerk(werkBase, 93);
  writeRun({ runId: 'r-old', card: 93, role: 'kade', go: false, phase: 'failed', startedAt: 't' }, runsDir);
  const sink: SpawnCall[] = [];
  await launchWerk(runsDir, werkBase, sink, 93);
  const act = sink.find((c) => c.command === 'bash');
  assert.equal(act?.opts.env?.WERK_RESUME, undefined, 'no proofs, no resume claim');
});

// #3664 — the two runner defects the #3660 land surfaced, red-first:
//
//  1. A go:true poll after the detached land FINISHED (pid dead, WERK_EXIT on
//     disk) must report the terminal truth ('landed'), never treat the record as
//     stale-running and RELAUNCH. The live #3660 failure: the post-land poll
//     spawned a spurious third act run that failed "no werk" and OVERWROTE the
//     landed run-on-record. Root: executeChorusWerkLand never reconciled the
//     sentinel before deciding, so a finished run read as running+dead-pid → start.
//
//  2. Failure evidence must survive a relaunch. The per-CARD log was truncated
//     (`> log`) by every new start, so the first failed land's reason was
//     unrecoverable. Now each run writes its own runId-keyed log and the record
//     carries `logFile`; a retry never destroys the prior run's evidence.
//
//  3. A land start with NO werk worktree on disk is refused typed (no-werk)
//     instead of spawning an act run that is guaranteed to fail and overwrite
//     the record.
// @test-type: unit — signal is fixture-data: temp runsDir + temp werk-base per test, spawn is a captured stub (hermetic; no live werk, no real act)

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type SpawnFn } from '../src/server';
import { writeRun, readRun, runLogPath } from '../src/werk-run-store';
import type { WerkRun } from '../src/werk-run-state';

type SpawnCall = { command: string; args: string[]; opts: { env?: Record<string, string>; detached?: boolean; stdio?: string } };

function captureSpawn(sink: SpawnCall[]): SpawnFn {
  return ((command: string, args: string[], opts: SpawnCall['opts'] = {}) => {
    sink.push({ command, args, opts });
    return { pid: 4242, unref() {} };
  }) as unknown as SpawnFn;
}

async function withServer(
  fn: (client: Client, runsDir: string) => Promise<void>,
  sink: SpawnCall[],
  werkBase?: string,
) {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-land-poll-'));
  const origBase = process.env.CHORUS_WERK_BASE;
  if (werkBase !== undefined) process.env.CHORUS_WERK_BASE = werkBase;
  const server = buildMcpServer(() => 'wren', { spawnFn: captureSpawn(sink), runsDir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'werk-land-poll-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    await fn(client, runsDir);
  } finally {
    await client.close();
    await server.close();
    if (origBase === undefined) delete process.env.CHORUS_WERK_BASE;
    else process.env.CHORUS_WERK_BASE = origBase;
    fs.rmSync(runsDir, { recursive: true, force: true });
  }
}

const DEAD_PID = 999999;

function seedRun(runsDir: string, over: Partial<WerkRun>, logContent?: string): WerkRun {
  const run: WerkRun = {
    runId: 'seed-run', card: 3660, role: 'wren', go: true, phase: 'running',
    startedAt: new Date().toISOString(), pid: DEAD_PID, ...over,
  };
  if (logContent !== undefined) {
    const logFile = run.logFile ?? runLogPath(run.card, run.runId, runsDir);
    fs.writeFileSync(logFile, logContent);
    run.logFile = logFile;
  }
  writeRun(run, runsDir);
  return run;
}

function textOf(res: unknown): string {
  return (res as { content: Array<{ text: string }> }).content.map((c) => c.text).join('\n');
}

// ── Defect 2: the #3660 spurious-relaunch regression ──────────────────────────

test('go:true poll after the land FINISHED (dead pid + WERK_EXIT=0) reports landed — never relaunches', async () => {
  const sink: SpawnCall[] = [];
  await withServer(async (client, runsDir) => {
    seedRun(runsDir, { card: 3660, go: true, phase: 'running', pid: DEAD_PID },
      'merge…sync…deploy…accept…\nWERK_EXIT=0\n');
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 3660, go: true } });
    const text = textOf(res);
    assert.match(text, /"phase":"landed"/, `finished land reports landed, got: ${text}`);
    assert.match(text, /"attached":true/, 'attaches to the terminal truth');
    assert.equal(sink.length, 0, 'NEVER spawns a fresh act after a finished land (the #3660 spurious 3rd run)');
    assert.equal(readRun(3660, runsDir)?.phase, 'landed', 'the landed truth is persisted, not overwritten');
  }, sink, fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-empty-')));
});

test('no-go poll ALSO reports landed for a finished go-run (any poll shape tells the truth)', async () => {
  const sink: SpawnCall[] = [];
  await withServer(async (client, runsDir) => {
    seedRun(runsDir, { card: 3660, go: true, phase: 'running', pid: DEAD_PID },
      'merge…\nWERK_EXIT=0\n');
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 3660 } });
    assert.match(textOf(res), /"phase":"landed"/, 'the no-go poll reconciles to landed too');
    assert.equal(sink.length, 0, 'no spawn');
  }, sink, fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-empty-')));
});

// ── Defect 3 (guard): a blind land start is refused, not spawned ──────────────

test('go:true with NO record and NO werk worktree refuses typed no-werk — no doomed act spawn', async () => {
  const sink: SpawnCall[] = [];
  await withServer(async (client) => {
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 4242, go: true } });
    const text = textOf(res);
    assert.match(text, /no-werk/, `typed no-werk refusal, got: ${text}`);
    assert.equal(sink.length, 0, 'never spawns an act run that must fail "no werk"');
  }, sink, fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-empty-')));
});

test('go:true retry after a FAILED land with the worktree present DOES start (retry stays legitimate)', async () => {
  const sink: SpawnCall[] = [];
  const werkBase = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-'));
  fs.mkdirSync(path.join(werkBase, 'wren-3660'), { recursive: true });
  await withServer(async (client, runsDir) => {
    seedRun(runsDir, { card: 3660, go: true, phase: 'failed', failureReason: 'merge-hiccup' });
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 3660, go: true } });
    assert.match(textOf(res), /"phase":"running"/, 'retry launches');
    assert.equal(sink.length, 1, 'exactly one fresh act spawn for the retry');
  }, sink, werkBase);
});

// ── Defect 1: failure evidence survives a relaunch (per-run logs) ─────────────

test('a retry writes a DIFFERENT per-run log — the failed run\'s evidence is never truncated', async () => {
  const sink: SpawnCall[] = [];
  const werkBase = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-'));
  fs.mkdirSync(path.join(werkBase, 'wren-3660'), { recursive: true });
  await withServer(async (client, runsDir) => {
    const evidence = 'werk-demo: unit build ok\nFATAL: the real reason #3660 died\nWERK_EXIT=1\n';
    const failed = seedRun(runsDir, { card: 3660, go: true, phase: 'failed', failureReason: 'the real reason #3660 died' }, evidence);
    await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 3660, go: true } });
    assert.equal(sink.length, 1, 'retry spawned');
    const wrapped = sink[0].args[1];
    assert.ok(!wrapped.includes(`> "${failed.logFile}"`), 'the retry must NOT truncate the failed run\'s log');
    const newRecord = readRun(3660, runsDir);
    assert.ok(newRecord?.logFile, 'new record carries its own per-run logFile');
    assert.notEqual(newRecord?.logFile, failed.logFile, 'runId-keyed log: each run gets its own file');
    assert.equal(fs.readFileSync(failed.logFile!, 'utf8'), evidence, 'prior evidence intact on disk');
  }, sink, werkBase);
});

test('reconcile reads the RECORD\'s own log and extracts the failure reason from it', async () => {
  const sink: SpawnCall[] = [];
  await withServer(async (client, runsDir) => {
    seedRun(runsDir, { card: 3661, go: false, phase: 'running', pid: DEAD_PID },
      'building…\nwerk-test: reason=coverage-floor\nWERK_EXIT=1\n');
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 3661 } });
    const text = textOf(res);
    assert.match(text, /"phase":"failed"/, 'finished failed run reports failed');
    assert.match(text, /coverage-floor/, `the child\'s real reason surfaces on the poll, got: ${text}`);
  }, sink, fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-empty-')));
});

// ── Held truth: exit 0 with a [HELD] witness is not a landed lie ──────────────

test('go-run that exited 0 but was HELD (structured WERK_HELD sentinel) reports failed+held, not landed', async () => {
  const sink: SpawnCall[] = [];
  await withServer(async (client, runsDir) => {
    seedRun(runsDir, { card: 3662, go: true, phase: 'running', pid: DEAD_PID },
      'merge skipped (not proven)\nWERK_HELD=held: witness missing [no-gather-reply] for this patch\nWERK_EXIT=0\n');
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 3662 } });
    const text = textOf(res);
    assert.ok(!/"phase":"landed"/.test(text), 'a held run must never read as landed');
    assert.match(text, /held/i, `held state surfaces to the poller, got: ${text}`);
  }, sink, fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-empty-')));
});

test('go-run exit 0 WITHOUT a WERK_HELD sentinel is landed — free-form log text (e.g. a debug line quoting "[HELD]") cannot misclassify', async () => {
  const sink: SpawnCall[] = [];
  await withServer(async (client, runsDir) => {
    seedRun(runsDir, { card: 3663, go: true, phase: 'running', pid: DEAD_PID },
      'echoing docs that mention [HELD] handling…\nmerge…deploy…accept…\nWERK_EXIT=0\n');
    const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'wren', card_id: 3663 } });
    assert.match(textOf(res), /"phase":"landed"/, 'only the structured sentinel means held (Silas gather)');
    assert.equal(sink.length, 0, 'no spawn');
  }, sink, fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-empty-')));
});

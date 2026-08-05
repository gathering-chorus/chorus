/**
 * #3751 — reopening a card left its old run pin in place, so chorus_werk replayed
 * the PREVIOUS cycle's outcome — including {phase:'landed', accepter:'jeff'} for
 * #3606 while its branch sat 10 commits ahead of origin/main with nothing merged.
 * A false green on the LAND path, carrying the human's name as accepter, caught
 * only by hand-checking git.
 *
 * Two mechanisms, both proven here:
 *  1. A pull starts a NEW cycle → the pull path clears the pin (store contract).
 *  2. Belt+suspenders at read time: a pin that predates the current worktree, or
 *     claims 'landed' while the werk is ahead of origin/main, is REFUSED typed —
 *     never reported as a phase. (isRunStale stays untouched: terminal phases
 *     remain final WITHIN a cycle; the cycle boundary is what these guard.)
 *
 * Negative proof (#3734): the violation fixtures below show the guard FIRING —
 * a stale landed pin + an unmerged branch is demonstrably refused, and a genuine
 * land is demonstrably reported. Both states reachable, separated.
 *
 * @test-type: unit — signal is fixture-data: temp runsDir + throwaway git repos
 * per test, spawn is a captured stub (hermetic; no live werk, no real act)
 */
import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { execFileSync } from 'node:child_process';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type SpawnFn } from '../src/server';
import { readRun, writeRun, clearRun, isRunStale, verifyPinIntegrity, archiveRun } from '../src/werk-run-store';
import type { WerkRun } from '../src/werk-run-state';

/** The exact #3606 pin, reconstructed from the archived record (3606.json.landed-20260706). */
const landedPin = (card: number, over: Partial<WerkRun> = {}): WerkRun => ({
  runId: `${card}-kade-30553-land`,
  card,
  role: 'kade',
  go: true,
  phase: 'landed',
  startedAt: '2026-07-06T14:53:02.760Z',
  pid: 49071,
  ...over,
});

const git = (dir: string, ...args: string[]): string =>
  execFileSync('git', ['-C', dir, ...args], { encoding: 'utf8' }).trim();

/** A throwaway "werk": a git repo with an origin/main ref and `ahead` commits past it. */
function makeWerk(base: string, name: string, ahead: number): string {
  const dir = path.join(base, name);
  fs.mkdirSync(dir, { recursive: true });
  execFileSync('git', ['init', '-q', dir]);
  git(dir, 'config', 'user.email', 'test@test');
  git(dir, 'config', 'user.name', 'test');
  fs.writeFileSync(path.join(dir, 'f.txt'), 'base');
  git(dir, 'add', '.');
  git(dir, 'commit', '-q', '-m', 'base');
  git(dir, 'update-ref', 'refs/remotes/origin/main', 'HEAD');
  for (let i = 0; i < ahead; i++) {
    fs.writeFileSync(path.join(dir, 'f.txt'), `ahead-${i}`);
    git(dir, 'add', '.');
    git(dir, 'commit', '-q', '-m', `unmerged-${i}`);
  }
  return dir;
}

describe('#3751 store contract — a run pin does not survive a new cycle', () => {
  let dir: string;
  beforeEach(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-runs-3751-')); });
  afterEach(() => { fs.rmSync(dir, { recursive: true, force: true }); });

  test('THE BUG, documented: a landed pin is NOT stale by the existing rule — terminal phases are final', () => {
    // Not a complaint about isRunStale — it is WHY clearing/verification must
    // happen at the cycle boundary. Loosening this rule would make terminal
    // phases racy within a cycle, a worse trade.
    assert.equal(isRunStale(landedPin(3606), Date.parse('2026-08-04T13:57:00Z')), false);
  });

  test('a landed pin survives indefinitely without an explicit clear', () => {
    writeRun(landedPin(3606), dir);
    const still = readRun(3606, dir);
    assert.ok(still);
    assert.equal(still.phase, 'landed');
    assert.equal(still.startedAt, '2026-07-06T14:53:02.760Z');
  });

  test('clearRun removes it, so the next cycle has no phase to replay', () => {
    writeRun(landedPin(3606), dir);
    assert.ok(readRun(3606, dir));
    clearRun(3606, dir);
    assert.equal(readRun(3606, dir), null);
  });

  test('clearRun is best-effort — a missing pin is not an error (a pull must not fail on it)', () => {
    assert.equal(readRun(4242, dir), null);
    assert.doesNotThrow(() => clearRun(4242, dir));
  });

  test('clearing one card does not disturb another role/card mid-flight', () => {
    writeRun(landedPin(3606), dir);
    writeRun({ runId: '3747-wren-1', card: 3747, role: 'wren', go: false, phase: 'running', startedAt: '2026-08-04T19:00:00.000Z', pid: 1 } as WerkRun, dir);
    clearRun(3606, dir);
    assert.equal(readRun(3606, dir), null);
    assert.equal(readRun(3747, dir)?.phase, 'running');
  });

  test('AC5: archiveRun supersedes without destroying — archives are invisible to readRun', () => {
    writeRun(landedPin(3606), dir);
    const dest = archiveRun(3606, 'stale-test', dir);
    assert.ok(dest && dest.endsWith('3606.json.stale-test'));
    assert.equal(readRun(3606, dir), null); // no longer readable as THE pin
    assert.ok(fs.existsSync(dest)); // but the forensic record survives
    const forensic = JSON.parse(fs.readFileSync(dest, 'utf8')) as WerkRun;
    assert.equal(forensic.phase, 'landed'); // content intact
  });

  test('archiveRun with nothing to archive returns null, never throws', () => {
    assert.equal(archiveRun(4242, 'stale-x', dir), null);
  });
});

describe('#3751 verifyPinIntegrity — the two violations, on real git fixtures', () => {
  let base: string;
  beforeEach(() => { base = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-3751-')); });
  afterEach(() => { fs.rmSync(base, { recursive: true, force: true }); });

  test('NEGATIVE PROOF: landed pin + werk ahead of origin/main → landed-but-unmerged', () => {
    const werk = makeWerk(base, 'kade-3606', 1);
    const v = verifyPinIntegrity(landedPin(3606, { startedAt: new Date().toISOString() }), werk);
    assert.equal(v.ok, false);
    assert.equal(!v.ok && v.reason, 'landed-but-unmerged');
  });

  test('NEGATIVE PROOF: pin older than the worktree → pin-predates-cycle (any phase)', () => {
    const werk = makeWerk(base, 'kade-3606', 0);
    // July pin against a worktree created just now — the reopen shape exactly.
    const v = verifyPinIntegrity(landedPin(3606, { phase: 'presented', go: false }), werk);
    assert.equal(v.ok, false);
    assert.equal(!v.ok && v.reason, 'pin-predates-cycle');
  });

  test('genuine landed pin, werk merged (0 ahead), fresh startedAt → ok', () => {
    const werk = makeWerk(base, 'kade-3700', 0);
    const v = verifyPinIntegrity(landedPin(3700, { startedAt: new Date().toISOString() }), werk);
    assert.equal(v.ok, true);
  });

  test('genuine land AFTER teardown: no werk on disk → ok (landed-with-no-werk is the normal terminal state)', () => {
    const v = verifyPinIntegrity(landedPin(3700), path.join(base, 'kade-3700'));
    assert.equal(v.ok, true);
  });

  test('uncertainty degrades to ok: a werk dir that is not a git repo never refuses', () => {
    const notRepo = path.join(base, 'kade-4242');
    fs.mkdirSync(notRepo, { recursive: true });
    // no .git at all → neither check can gather evidence → ok
    const v = verifyPinIntegrity(landedPin(4242, { startedAt: new Date().toISOString() }), notRepo);
    assert.equal(v.ok, true);
  });
});

// ---------- server level: chorus_werk REFUSES, typed — the #3606 replay cannot recur ----------

type SpawnCall = { command: string; args: string[] };
function captureSpawn(sink: SpawnCall[]): SpawnFn {
  return ((command: string, args: string[]) => {
    sink.push({ command, args });
    return { pid: 4242, unref() {} };
  }) as unknown as SpawnFn;
}

async function withServer(
  fn: (client: Client, runsDir: string, werkBase: string) => Promise<void>,
  sink: SpawnCall[],
) {
  const runsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-runs-3751-srv-'));
  const werkBase = fs.mkdtempSync(path.join(os.tmpdir(), 'werk-base-3751-srv-'));
  const origBase = process.env.CHORUS_WERK_BASE;
  process.env.CHORUS_WERK_BASE = werkBase;
  const server = buildMcpServer(() => 'kade', { spawnFn: captureSpawn(sink), runsDir, cardsPath: '/fake/cards' });
  const [ct, st] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'pull-clears-run-pin-test', version: '1.0' });
  await Promise.all([server.connect(st), client.connect(ct)]);
  try {
    await fn(client, runsDir, werkBase);
  } finally {
    await client.close();
    await server.close();
    if (origBase === undefined) delete process.env.CHORUS_WERK_BASE;
    else process.env.CHORUS_WERK_BASE = origBase;
    fs.rmSync(runsDir, { recursive: true, force: true });
    fs.rmSync(werkBase, { recursive: true, force: true });
  }
}

async function callWerk(client: Client, card_id: number, go = false): Promise<Record<string, unknown>> {
  const res = await client.callTool({ name: 'chorus_werk', arguments: { role: 'kade', card_id, ...(go ? { go: true } : {}) } });
  const content = (res as { content: Array<{ type: string; text: string }> }).content;
  return JSON.parse(content[0].text) as Record<string, unknown>;
}

describe('#3751 chorus_werk — stale pin is refused typed, never replayed as a phase', () => {
  test('NEGATIVE PROOF (the #3606 shape): stale landed pin + unmerged werk → refusal, pin archived', async () => {
    const sink: SpawnCall[] = [];
    await withServer(async (client, runsDir, werkBase) => {
      makeWerk(werkBase, 'kade-3606', 1); // 1 commit ahead, NOTHING merged
      writeRun(landedPin(3606), runsDir); // the July pin, left by the reopen

      const out = await callWerk(client, 3606);

      // Never {ok:true, phase:'landed'} — the false land report cannot recur.
      assert.equal(out.ok, false);
      assert.equal(out.refusal, 'stale-run-pin');
      assert.equal(out.reason, 'pin-predates-cycle'); // July pin vs a just-made worktree
      assert.equal(out.phase, undefined); // no phase is reported from the bad pin

      // The pin is superseded (archived), not destroyed: readRun is empty,
      // the forensic file exists, and the next invoke can start fresh.
      assert.equal(readRun(3606, runsDir), null);
      const archives = fs.readdirSync(runsDir).filter((f) => f.startsWith('3606.json.stale-'));
      assert.equal(archives.length, 1);

      // The spine hears about it.
      const spine = sink.find((c) => c.args.some((a) => String(a).includes('werk.pin.stale')));
      assert.ok(spine, 'werk.pin.stale spine event emitted');
    }, sink);
  });

  test('NEGATIVE PROOF on the LAND path: go:true against the stale pin refuses the same way', async () => {
    const sink: SpawnCall[] = [];
    await withServer(async (client, runsDir, werkBase) => {
      makeWerk(werkBase, 'kade-3606', 1);
      writeRun(landedPin(3606), runsDir);

      const out = await callWerk(client, 3606, true);

      assert.equal(out.ok, false);
      assert.equal(out.refusal, 'stale-run-pin');
      assert.equal(readRun(3606, runsDir), null);
    }, sink);
  });

  test('fresh landed-but-unmerged pin (the retry-a-land shape) → landed-but-unmerged refusal', async () => {
    const sink: SpawnCall[] = [];
    await withServer(async (client, runsDir, werkBase) => {
      makeWerk(werkBase, 'kade-3606', 1);
      writeRun(landedPin(3606, { startedAt: new Date().toISOString() }), runsDir);

      const out = await callWerk(client, 3606);

      assert.equal(out.ok, false);
      assert.equal(out.refusal, 'stale-run-pin');
      assert.equal(out.reason, 'landed-but-unmerged');
    }, sink);
  });

  test('GENUINE land (werk torn down) is still reported — the guard separates the two states', async () => {
    const sink: SpawnCall[] = [];
    await withServer(async (client, runsDir) => {
      // Real terminal state after accept: pin says landed, worktree is gone.
      writeRun(landedPin(3700), runsDir);

      const out = await callWerk(client, 3700);

      assert.equal(out.ok, true);
      assert.equal(out.phase, 'landed');
      assert.equal(out.attached, true);
      assert.ok(readRun(3700, runsDir), 'genuine pin untouched');
    }, sink);
  });
});

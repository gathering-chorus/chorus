/**
 * #4420 — a werk run can be cancelled or paused (Jeff 2026-10-02). Real files in
 * a temp runs dir; process signals through fake deps (no live act).
 */
// @test-type: unit — temp dirs and fake signal deps; no act, no services, no network
import { test, describe } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { cancelRun, pauseRun, resumeRun, isHeld, isCardVerb, CANCEL_MARKER, type ControlDeps } from '../src/werk-run-control';
import type { WerkRun } from '../src/werk-run-state';

const run = (over: Partial<WerkRun> = {}): WerkRun => ({
  runId: 'r1', card: 4420, role: 'kade', go: false, phase: 'running', pid: 777,
  startedAt: '2026-10-06T14:00:00Z', ...over,
});

function fakeDeps(aliveAfterTerm: boolean) {
  let alive = true;
  const signals: Array<[number, string]> = [];
  const written: WerkRun[] = [];
  const logs: string[] = [];
  const deps: ControlDeps = {
    killGroup: (pid, sig) => { signals.push([pid, sig]); if (sig === 'SIGKILL' || !aliveAfterTerm) alive = false; },
    isAlive: () => alive,
    startedAt: () => new Date('2026-10-06T14:00:10Z'),
    writeRun: (r) => written.push(r),
    appendLog: (_f, line) => logs.push(line),
    sleep: async () => {},
    now: () => new Date('2026-10-06T14:05:00Z'),
    cardVerbs: () => [],
    killPid: () => {},
  };
  return { deps, signals, written, logs };
}

const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'werk-4420-'));

describe('cancel', () => {
  test('stops the whole process group, records cancelled, and marks the run log', async () => {
    const f = fakeDeps(false);
    const res = await cancelRun(run({ logFile: '/x.log' }), f.deps, tmp());
    assert.equal(res.ok && res.phase, 'cancelled');
    assert.deepEqual(f.signals, [[777, 'SIGTERM']]);
    assert.equal(f.written[0].phase, 'cancelled');
    assert.deepEqual(f.logs, [`${CANCEL_MARKER}\n`]);
  });

  test('a group that ignores TERM is KILLed', async () => {
    const f = fakeDeps(true);
    await cancelRun(run(), f.deps, tmp(), 1000);
    assert.deepEqual(f.signals.map((s) => s[1]), ['SIGTERM', 'SIGKILL']);
  });

  // NEGATIVE PROOF — a finished run is not "cancelled" over its real outcome.
  test('a run that is not running is refused and left as it was', async () => {
    const f = fakeDeps(false);
    const res = await cancelRun(run({ phase: 'presented' }), f.deps, tmp());
    assert.equal(res.ok, false);
    assert.equal(f.signals.length + f.written.length, 0);
  });

  test('a cancel clears a pause so nothing waits on a dead run', async () => {
    const dir = tmp();
    pauseRun(run(), dir);
    await cancelRun(run(), fakeDeps(false).deps, dir);
    assert.equal(isHeld(4420, dir), false);
  });
});

describe("cancel signals only this run's own process", () => {
  // NEGATIVE PROOF — the 10-06 #4214 pin: Sep 19 start, its pid now held by another process.
  test('a pid that started long after the run is never signalled', async () => {
    const f = fakeDeps(false);
    const res = await cancelRun(run({ startedAt: '2026-09-19T23:52:10Z' }), f.deps, tmp());
    assert.equal(f.signals.length, 0);
    assert.equal(res.ok && res.phase, 'cancelled', 'the run record still says cancelled');
  });
});

// Silas, 2026-10-11 17:30 on #4422: cancel said "act and its children stopped"
// while werk-build (pid 50896) ran on and held the werk lock; the next run failed
// at build. act reaches werk-build through chorus-api's MCP server, so the verb
// is chorus-api's child, never in act's process group.
describe('cancel stops the card\'s verbs outside act\'s group, and says so only when they are gone', () => {
  function withVerb(survives: boolean) {
    const f = fakeDeps(false);
    let verbAlive = true;
    const verbSignals: Array<[number, string]> = [];
    f.deps.cardVerbs = () => (verbAlive ? [{ pid: 50896, command: '/Users/j/.chorus/bin/werk-build 4420 kade' }] : []);
    f.deps.killPid = (pid, sig) => { verbSignals.push([pid, sig]); if (!survives) verbAlive = false; };
    return { ...f, verbSignals };
  }

  test('a werk-build spawned by chorus-api is stopped with the run', async () => {
    const f = withVerb(false);
    const res = await cancelRun(run(), f.deps, tmp());
    assert.deepEqual(f.verbSignals, [[50896, 'SIGTERM']]);
    assert.equal(res.ok && res.phase, 'cancelled');
  });

  // NEGATIVE PROOF — the 10-11 shape: a verb that will not die is named, never "stopped".
  test('a verb still alive after TERM and KILL makes the cancel say so, naming it', async () => {
    const f = withVerb(true);
    const res = await cancelRun(run(), f.deps, tmp(), 1000);
    assert.deepEqual(f.verbSignals.map((s) => s[1]), ['SIGTERM', 'SIGKILL']);
    assert.equal(res.ok, false);
    assert.equal(!res.ok && res.refusal, 'still-running');
    assert.match(res.note, /50896/);
    assert.doesNotMatch(res.note, /stopped,/);
  });
});

describe('isCardVerb: which processes belong to this card', () => {
  test('a werk verb with this card and role', () => {
    assert.equal(isCardVerb('/Users/j/.chorus/bin/werk-build 4420 kade', 'kade', 4420), true);
    assert.equal(isCardVerb('/Users/j/.chorus/bin/werk-deploy env-up kade 4420', 'kade', 4420), true);
  });
  // NEGATIVE PROOF — another card, another role, or a file named like a verb is never signalled.
  test('never another card, another role, or a non-verb', () => {
    assert.equal(isCardVerb('/Users/j/.chorus/bin/werk-build 44200 kade', 'kade', 4420), false);
    assert.equal(isCardVerb('/Users/j/.chorus/bin/werk-build 4420 silas', 'kade', 4420), false);
    assert.equal(isCardVerb('vim werk-build 4420 kade', 'kade', 4420), false);
    assert.equal(isCardVerb('/opt/bin/node server.js 4420 kade', 'kade', 4420), false);
  });
});

describe('pause / resume', () => {
  test('pause writes the hold the pipeline waits on; resume removes it', () => {
    const dir = tmp();
    assert.equal(pauseRun(run(), dir).ok, true);
    assert.equal(isHeld(4420, dir), true);
    assert.equal(resumeRun(run(), dir).ok, true);
    assert.equal(isHeld(4420, dir), false);
  });

  test('pausing a run that is not running is refused', () => {
    const dir = tmp();
    assert.equal(pauseRun(run({ phase: 'failed' }), dir).ok, false);
    assert.equal(isHeld(4420, dir), false);
  });

  // the pipeline side: every step of werk.yml waits on the same hold file
  test('every step that does work waits on the hold first (werk.yml)', () => {
    const yml = fs.readFileSync(path.join(process.cwd(), '..', '..', '.github', 'workflows', 'werk.yml'), 'utf8');
    const waits = yml.split('\n').filter((l) => l.includes('.chorus/werk-runs/${CARD_ID}.hold'));
    assert.equal(waits.length, 14, 'commit push build test review deploy-werk env-up demo-fitness prove-live demo merge sync deploy-canonical accept');
  });
});

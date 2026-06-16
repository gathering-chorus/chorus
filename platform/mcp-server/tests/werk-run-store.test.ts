/**
 * #3443 AC7 — run-state persistence: the durable record a re-invoke reads to
 * attach instead of double-acting. Uses a temp dir; never touches the real
 * ~/.chorus/werk-runs.
 */
import { test, describe, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'fs';
import path from 'path';
import os from 'os';
import { readRun, writeRun, markPhase, clearRun, isRunStale, logPath, reconcileRunning } from '../src/werk-run-store';
import type { WerkRun } from '../src/werk-run-state';

let dir: string;
before(() => { dir = mkdtempSync(path.join(os.tmpdir(), 'werk-runs-')); });
after(() => { rmSync(dir, { recursive: true, force: true }); });

const run = (over: Partial<WerkRun> = {}): WerkRun => ({
  runId: 'r1', card: 3443, role: 'wren', go: false, phase: 'running',
  startedAt: '2026-06-15T23:40:00Z', ...over,
});

describe('run-store persistence', () => {
  test('missing record reads as null (→ start-fresh, never throws)', () => {
    assert.equal(readRun(9999, dir), null);
  });

  test('write then read round-trips the record', () => {
    writeRun(run({ card: 100, phase: 'running', pid: 4242 }), dir);
    const r = readRun(100, dir);
    assert.equal(r?.phase, 'running');
    assert.equal(r?.pid, 4242);
    assert.equal(r?.card, 100);
  });

  test('markPhase advances phase + carries the failure reason, preserving identity', () => {
    writeRun(run({ card: 101, runId: 'keep-me', phase: 'running' }), dir);
    const next = markPhase(101, 'failed', { failureReason: 'round-expired' }, dir);
    assert.equal(next?.phase, 'failed');
    assert.equal(next?.failureReason, 'round-expired');
    assert.equal(next?.runId, 'keep-me'); // identity preserved across the transition
    assert.equal(readRun(101, dir)?.phase, 'failed'); // persisted
  });

  test('markPhase on a missing record is a no-op null (cannot fabricate a run)', () => {
    assert.equal(markPhase(404, 'landed', {}, dir), null);
  });

  test('clearRun removes the record (next card starts clean)', () => {
    writeRun(run({ card: 102 }), dir);
    clearRun(102, dir);
    assert.equal(readRun(102, dir), null);
  });
});

describe('reconcileRunning — poll-time transition from the detached run\'s log (#3458)', () => {
  test('running + log WERK_EXIT=0 -> presented (the detached act finished clean)', () => {
    writeRun(run({ card: 200, phase: 'running', pid: process.pid }), dir);
    writeFileSync(logPath(200, dir), '…build…\n[presented] up\nWERK_EXIT=0\n');
    const r = reconcileRunning(200, dir);
    assert.equal(r?.phase, 'presented');
    assert.equal(readRun(200, dir)?.phase, 'presented'); // persisted
  });

  test('LAND run (go:true) + WERK_EXIT=0 -> landed, not presented (go-aware terminal)', () => {
    writeRun(run({ card: 204, phase: 'running', go: true, pid: process.pid }), dir);
    writeFileSync(logPath(204, dir), 'merge…sync…deploy…\nWERK_EXIT=0\n');
    assert.equal(reconcileRunning(204, dir)?.phase, 'landed');
  });

  test('running + log WERK_EXIT=1 -> failed, with the child reason from the log', () => {
    writeRun(run({ card: 201, phase: 'running', pid: process.pid }), dir);
    writeFileSync(logPath(201, dir), 'Failure - Main merge\nwerk-merge: reason=patch-id-changed\nWERK_EXIT=1\n');
    const r = reconcileRunning(201, dir);
    assert.equal(r?.phase, 'failed');
    assert.equal(r?.failureReason, 'patch-id-changed');
  });

  test('running + no sentinel yet -> stays running (act still going)', () => {
    writeRun(run({ card: 202, phase: 'running', pid: process.pid }), dir);
    writeFileSync(logPath(202, dir), '…build still going…\n');
    assert.equal(reconcileRunning(202, dir)?.phase, 'running');
  });

  test('already presented -> unchanged (only running reconciles)', () => {
    writeRun(run({ card: 203, phase: 'presented' }), dir);
    assert.equal(reconcileRunning(203, dir)?.phase, 'presented');
  });

  test('missing run -> null (nothing to reconcile)', () => {
    assert.equal(reconcileRunning(999, dir), null);
  });
});

describe('isRunStale — a dead/old running record is detected (#3458 belt+suspenders)', () => {
  test('running with a dead pid -> stale (the lost-terminal-write case)', () => {
    assert.equal(isRunStale(run({ phase: 'running', pid: 999999 })), true);
  });

  test('running with THIS live pid + within TTL -> not stale (a genuine run is never stranded)', () => {
    const started = '2026-06-16T12:00:00Z';
    const now = Date.parse('2026-06-16T12:01:00Z'); // 1 min in, well within TTL
    assert.equal(isRunStale(run({ phase: 'running', pid: process.pid, startedAt: started }), now), false);
  });

  test('running past the TTL -> stale (no act run lasts this long)', () => {
    const now = Date.parse('2026-06-16T12:00:00Z');
    // started 31 min ago, default 30-min TTL, pid alive (so only TTL triggers)
    const r = run({ phase: 'running', pid: process.pid, startedAt: '2026-06-16T11:29:00Z' });
    assert.equal(isRunStale(r, now), true);
  });

  test('presented record is never "stale" (only running can be)', () => {
    assert.equal(isRunStale(run({ phase: 'presented', pid: 999999 })), false);
  });
});

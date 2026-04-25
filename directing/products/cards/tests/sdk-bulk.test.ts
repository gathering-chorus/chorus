/**
 * sdk.ts bulk / snapshot tests (#2241 wave 2 pt 3).
 *
 * Covers bulkSequenceTag, bulkMove, and snapshotBoard — the batch helpers
 * used for housekeeping and audit. Uses a mock BoardClient + temp
 * snapshot directory to stay hermetic.
 */

import * as fs from 'fs';
import type { BoardClient } from '../src/client';
import { bulkSequenceTag, bulkMove, snapshotBoard } from '../src/sdk';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tagShouldThrow?: (index: number) => Error | null;
  moveShouldThrow?: (index: number) => Error | null;
  snapshotReturn: { board: string; timestamp: string; tasks: Array<{ index: number; title: string }> } = {
    board: 'gathering',
    timestamp: '2026-04-19T10:00:00Z',
    tasks: [
      { index: 1, title: 'first' },
      { index: 2, title: 'second' },
    ],
  };

  async tag(index: number, category: string, value: string): Promise<void> {
    this.calls.push({ method: 'tag', args: [index, category, value] });
    if (this.tagShouldThrow) {
      const err = this.tagShouldThrow(index);
      if (err) throw err;
    }
  }

  async move(index: number, status: string): Promise<void> {
    this.calls.push({ method: 'move', args: [index, status] });
    if (this.moveShouldThrow) {
      const err = this.moveShouldThrow(index);
      if (err) throw err;
    }
  }

  async snapshot(): Promise<{ board: string; timestamp: string; tasks: Array<{ index: number; title: string }> }> {
    this.calls.push({ method: 'snapshot', args: [] });
    return this.snapshotReturn;
  }
}

function asBoardClient(m: MockClient): BoardClient {
  return m as unknown as BoardClient;
}

function silence() {
  const origLog = console.log;
  const origErr = console.error;
  const logs: string[] = [];
  const errs: string[] = [];
  console.log = (...a) => logs.push(a.join(' '));
  console.error = (...a) => errs.push(a.join(' '));
  return {
    logs, errs,
    restore: () => { console.log = origLog; console.error = origErr; },
  };
}

function interceptExit() {
  const calls: number[] = [];
  const orig = process.exit;
  process.exit = ((code?: number) => {
    calls.push(code ?? 0);
    throw new Error(`process.exit(${code})`);
  }) as typeof process.exit;
  return { calls, restore: () => { process.exit = orig; } };
}

describe('bulkSequenceTag', () => {
  it('tags each id with the given sequence and reports counts', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await bulkSequenceTag(asBoardClient(mock), [1, 2, 3], 'quality');
    } finally {
      cap.restore();
    }
    const tagCalls = mock.calls.filter((c) => c.method === 'tag');
    expect(tagCalls).toHaveLength(3);
    expect(tagCalls.every((c) => c.args[1] === 'sequence' && c.args[2] === 'quality')).toBe(true);
    expect(cap.logs.join('\n')).toMatch(/Tagged 3 card/);
  });

  it('tolerates 409-already-tagged as skipped', async () => {
    const mock = new MockClient();
    mock.tagShouldThrow = (index) => index === 2 ? new Error('409 already tagged') : null;
    const cap = silence();
    try {
      await bulkSequenceTag(asBoardClient(mock), [1, 2, 3], 'quality');
    } finally {
      cap.restore();
    }
    expect(cap.logs.join('\n')).toMatch(/Tagged 2 card.*1 already tagged/);
  });

  it('invalid sequence → process.exit(1)', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await bulkSequenceTag(asBoardClient(mock), [1], 'not-a-real-seq').catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    expect(cap.errs.join('\n')).toMatch(/Unknown sequence/);
  });

  it('non-409 failure prints error but continues', async () => {
    const mock = new MockClient();
    mock.tagShouldThrow = (index) => index === 2 ? new Error('500 internal') : null;
    const cap = silence();
    try {
      await bulkSequenceTag(asBoardClient(mock), [1, 2, 3], 'quality');
    } finally {
      cap.restore();
    }
    expect(cap.errs.join('\n')).toMatch(/#2.*500/);
    expect(cap.logs.join('\n')).toMatch(/Tagged 2 card/);
  });
});

describe('bulkMove', () => {
  it('moves each id to the target status', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await bulkMove(asBoardClient(mock), [1, 2, 3], 'Later');
    } finally {
      cap.restore();
    }
    const moveCalls = mock.calls.filter((c) => c.method === 'move');
    expect(moveCalls.map((c) => c.args[0])).toEqual([1, 2, 3]);
    expect(moveCalls.every((c) => c.args[1] === 'Later')).toBe(true);
  });

  it('prints per-card status line and totals', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await bulkMove(asBoardClient(mock), [1, 2], 'Later');
    } finally {
      cap.restore();
    }
    expect(cap.logs.some((l) => /^ {2}#1 → Later/.test(l))).toBe(true);
    expect(cap.logs.some((l) => /Moved 2 card/.test(l))).toBe(true);
  });

  it('continues after a failed move and counts failures', async () => {
    const mock = new MockClient();
    mock.moveShouldThrow = (index) => index === 2 ? new Error('conflict') : null;
    const cap = silence();
    try {
      await bulkMove(asBoardClient(mock), [1, 2, 3], 'Done');
    } finally {
      cap.restore();
    }
    expect(cap.errs.join('\n')).toMatch(/#2.*conflict/);
    expect(cap.logs.join('\n')).toMatch(/Moved 2 card.*1 failed/);
  });
});

describe('snapshotBoard', () => {
  it('writes JSON to SNAPSHOT_DIR and returns the path', async () => {
    const mock = new MockClient();
    const cap = silence();
    let out: string | undefined;
    try {
      out = await snapshotBoard(asBoardClient(mock));
    } finally {
      cap.restore();
    }
    expect(out).toBeDefined();
    expect(out).toMatch(/board-snapshot-gathering\.json$/);
    expect(fs.existsSync(out!)).toBe(true);
    // Clean up the file so repeated runs don't accumulate (best effort).
    try { fs.unlinkSync(out!); } catch { /* ignore */ }
  });

  it('log line reports the task count', async () => {
    const mock = new MockClient();
    mock.snapshotReturn = {
      board: 'gathering', timestamp: 't', tasks: new Array(7).fill(null).map((_, i) => ({ index: i, title: `t${i}` })),
    };
    const cap = silence();
    try {
      const p = await snapshotBoard(asBoardClient(mock));
      try { fs.unlinkSync(p); } catch { /* ignore */ }
    } finally {
      cap.restore();
    }
    expect(cap.logs.join('\n')).toMatch(/\(7 tasks\)/);
  });

  it('consumes nothing new when tasks array is empty', async () => {
    const mock = new MockClient();
    mock.snapshotReturn = { board: 'gathering', timestamp: 't', tasks: [] };
    const cap = silence();
    try {
      const p = await snapshotBoard(asBoardClient(mock));
      try { fs.unlinkSync(p); } catch { /* ignore */ }
    } finally {
      cap.restore();
    }
    expect(cap.logs.join('\n')).toMatch(/\(0 tasks\)/);
  });
});

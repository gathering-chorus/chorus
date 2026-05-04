/**
 * doneCard verify-after-move regression tests (#2707).
 *
 * The bug: client.done(index) returns void on success, but the board move
 * could silently fail to apply (transient API timeout, async race, etc.).
 * doneCard then logged "Done: #N" and emitted card.accepted — for a card
 * still in WIP. /acp consumed the (false) success and the spine event lied.
 *
 * Fix: doneCard re-reads the card after the move and asserts Status=Done.
 * If verify fails, retry once. If retry also fails, throw (CLI exits non-zero).
 */

import { __setTestPaths, __resetTestPaths, doneCard } from '../src/sdk';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

function silence() {
  const origLog = console.log;
  const origErr = console.error;
  const logs: string[] = [];
  const errs: string[] = [];
  console.log = (...a) => logs.push(a.join(' '));
  console.error = (...a) => errs.push(a.join(' '));
  return { logs, errs, restore: () => { console.log = origLog; console.error = origErr; } };
}

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  // Sequence of status values returned by successive view() calls.
  // Lets a test simulate "done() succeeded but board still shows WIP on
  // first verify, then settles on second verify" (transient cache) vs
  // "stays WIP across both verifies" (real silent failure).
  viewSequence: ('WIP' | 'Done' | 'Next')[] = ['WIP'];
  viewIndex = 0;

  async done(_index: number) { this.calls.push({ method: 'done', args: [_index] }); }

  async view(index: number): Promise<BoardTask> {
    this.calls.push({ method: 'view', args: [index] });
    const status = this.viewSequence[Math.min(this.viewIndex, this.viewSequence.length - 1)];
    this.viewIndex++;
    return {
      index, title: `task-${index}`, description: '', status,
      owner: 'Kade', priority: 'P1', domains: ['type:chore'], apiId: index + 1000,
      created: '2026-04-19T10:00:00Z', updated: '2026-04-19T10:00:00Z', done: status === 'Done',
    } as unknown as BoardTask;
  }
  async comment(_index: number, _text: string) { this.calls.push({ method: 'comment', args: [_index, _text] }); }
}

describe('#2707 doneCard verify-after-move', () => {
  let tmpDir: string;
  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'cards-done-verify-'));
    __setTestPaths({
      snapshotDir: path.join(tmpDir, 'snapshot'),
      workflowsActiveDir: path.join(tmpDir, 'wf-active'),
      workflowsArchiveDir: path.join(tmpDir, 'wf-archive'),
      briefDirs: { wren: path.join(tmpDir, 'briefs') },
    });
    fs.mkdirSync(path.join(tmpDir, 'briefs'), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, 'snapshot'), { recursive: true });
  });
  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  test('AC1+AC2 — silent done failure (status stays WIP across both verifies) → throws', async () => {
    const client = new MockClient() as unknown as BoardClient;
    (client as unknown as MockClient).viewSequence = ['WIP', 'WIP', 'WIP']; // initial-read + two verifies
    const s = silence();
    try {
      await expect(doneCard(client, 2691)).rejects.toThrow(/board did not move card to Done/i);
    } finally {
      s.restore();
    }
    expect(s.logs.some((l) => l.startsWith('Done: #2691'))).toBe(false);
  });

  test('AC2 — first verify shows WIP (transient), second shows Done → succeeds', async () => {
    const client = new MockClient() as unknown as BoardClient;
    (client as unknown as MockClient).viewSequence = ['WIP', 'WIP', 'Done']; // initial-read + first-verify-fails + retry-verify-pass
    const s = silence();
    try {
      await doneCard(client, 2692);
    } finally {
      s.restore();
    }
    expect(s.logs.some((l) => l.startsWith('Done: #2692'))).toBe(true);
  });

  test('AC1 — happy path (done succeeds, first verify shows Done) → no retry, no error', async () => {
    const client = new MockClient() as unknown as BoardClient;
    (client as unknown as MockClient).viewSequence = ['WIP', 'Done']; // initial-read + first-verify-pass
    const s = silence();
    try {
      await doneCard(client, 2700);
    } finally {
      s.restore();
    }
    expect(s.logs.some((l) => l.startsWith('Done: #2700'))).toBe(true);
    const viewCalls = (client as unknown as MockClient).calls.filter((c) => c.method === 'view').length;
    expect(viewCalls).toBe(3); // initial title-read + demo-gate-check + 1 verify (no retry)
  });
});

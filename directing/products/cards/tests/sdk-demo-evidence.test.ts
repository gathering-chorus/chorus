/**
 * checkDemoEvidence coverage (#2241 wave 12).
 *
 * doneCard calls checkDemoEvidence internally. This test exercises three
 * paths — --proven bypass, missing evidence, and present brief file — to
 * cover the scan loop and exit branches.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  __setTestPaths,
  __resetTestPaths,
  doneCard,
} from '../src/sdk';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

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
  // #2707 — mock now models a working board: done() flips the status
  // returned by subsequent view() calls. Before, view() always returned WIP,
  // which masked the silent-done-failure bug doneCard now catches.
  doneCalled = new Set<number>();
  async done(index: number) { this.calls.push({ method: 'done', args: [index] }); this.doneCalled.add(index); }
  async view(index: number): Promise<BoardTask> {
    this.calls.push({ method: 'view', args: [index] });
    const status = this.doneCalled.has(index) ? 'Done' : 'WIP';
    return {
      index, title: `task-${index}`, description: '', status,
      owner: 'Kade', priority: 'P1', domains: [], apiId: index + 1000,
      created: '2026-04-19T10:00:00Z', updated: '2026-04-19T10:00:00Z', done: status === 'Done',
    } as unknown as BoardTask;
  }
  commentsByIndex: Map<number, Array<{ author: string; text: string }>> = new Map();
  async comments(index: number) {
    this.calls.push({ method: 'comments', args: [index] });
    return this.commentsByIndex.get(index) || [];
  }
}

function asBoardClient(m: MockClient): BoardClient { return m as unknown as BoardClient; }

describe('doneCard demo-evidence scan', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'sdk-demo-ev-'));
    __setTestPaths({
      briefDirs: {
        silas: path.join(tmp, 'silas', 'briefs'),
        kade: path.join(tmp, 'kade', 'briefs'),
        wren: path.join(tmp, 'wren', 'briefs'),
      },
    });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it('--proven override completes regardless of evidence', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await doneCard(asBoardClient(mock), 42, ['1815', '1898']);
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'done')?.args[0]).toBe(42);
  });

  it('no demo:preflight-pass comment → process.exit(1) blocks done (#2910 canonical)', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exitSpy = jest.spyOn(process, 'exit').mockImplementation(((code?: number) => {
      throw new Error(`exit:${code}`);
    }) as never);
    try {
      await expect(doneCard(asBoardClient(mock), 99)).rejects.toThrow('exit:1');
    } finally {
      cap.restore();
      exitSpy.mockRestore();
    }
    expect(mock.calls.find((c) => c.method === 'done')).toBeUndefined();
  });

  it('finds demo:preflight-pass comment via client.comments() (#2910 canonical)', async () => {
    const mock = new MockClient();
    mock.commentsByIndex.set(42, [
      { author: 'wren', text: 'demo:preflight-pass ac=3/3 — wren' },
    ]);
    const cap = silence();
    try {
      await doneCard(asBoardClient(mock), 42);
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'done')).toBeDefined();
  });
});

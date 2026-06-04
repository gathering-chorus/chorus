/**
 * cards-done is a board primitive, not the accept authority. Card 3227.
 *
 * Before card 3227, doneCard re-gated on a demo:preflight-pass comment. But
 * werk-accept already gates on the demo.verdict witness and then calls cards
 * done to finalize, so the card gated twice and died at the second gate even
 * after passing the first. That was the card 3222 gauntlet. Card 3227 makes one
 * gate the rule: the demo gate lives in werk-accept's demo_verdict_pass, and
 * cards done no longer blocks on demo evidence. Done is Jeff's call. These tests
 * assert the primitive transitions with no demo gate; the gate itself is covered
 * by werk-accept's own suite.
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
  doneCalled = new Set<number>();
  async done(index: number) { this.calls.push({ method: 'done', args: [index] }); this.doneCalled.add(index); }
  async comment(index: number, text: string) { this.calls.push({ method: 'comment', args: [index, text] }); }
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

describe('cards-done is a primitive with no demo gate', () => {
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

  it('completes when there is no demo evidence', async () => {
    const mock = new MockClient();
    const cap = silence();
    let refused = false;
    const spy = jest.spyOn(process, 'exit').mockImplementation(((code?: number) => {
      refused = true;
      throw new Error(`refused:${code}`);
    }) as never);
    try {
      await doneCard(asBoardClient(mock), 99);
    } finally {
      cap.restore();
      spy.mockRestore();
    }
    expect(refused).toBe(false);
    expect(mock.calls.find((c) => c.method === 'done')?.args[0]).toBe(99);
  });

  it('proven path records provenance and completes', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await doneCard(asBoardClient(mock), 42, ['1815', '1898']);
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'done')?.args[0]).toBe(42);
    expect(mock.calls.some((c) => c.method === 'comment')).toBe(true);
  });

  it('never emits the demo gate refusal', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await doneCard(asBoardClient(mock), 7);
    } finally { cap.restore(); }
    const refusalLogged = cap.errs.some((e) => {
      const lower = e.toLowerCase();
      return lower.includes('demo gate') || lower.includes('no demo evidence');
    });
    expect(refusalLogged).toBe(false);
    expect(mock.calls.find((c) => c.method === 'done')?.args[0]).toBe(7);
  });
});

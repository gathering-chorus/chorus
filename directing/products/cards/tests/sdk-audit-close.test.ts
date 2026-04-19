/**
 * auditClose diff tests (#2241 wave 4).
 *
 * Pre-populates a snapshot file via auditStart, then mutates the mock
 * client's tasks, then runs auditClose and asserts on the diff output:
 * newCards, newlyDone, retroactive, still-in-progress.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import type { BoardClient } from '../src/client';
import {
  __setTestPaths,
  __resetTestPaths,
  auditStart,
  auditClose,
} from '../src/sdk';

function mkTmp(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'cards-sdk-auditclose-'));
}

function silence() {
  const origLog = console.log;
  const origErr = console.error;
  const logs: string[] = [];
  console.log = (...a) => logs.push(a.join(' '));
  console.error = () => {};
  return {
    logs, restore: () => { console.log = origLog; console.error = origErr; },
  };
}

class MockClient {
  boardName = 'gathering';
  tasks: Array<Record<string, unknown>> = [];
  async snapshot(): Promise<{ board: string; timestamp: string; tasks: Array<Record<string, unknown>> }> {
    return { board: this.boardName, timestamp: new Date().toISOString(), tasks: this.tasks };
  }
  async list(): Promise<Array<Record<string, unknown>>> {
    return this.tasks;
  }
}

function asBoardClient(m: MockClient): BoardClient {
  return m as unknown as BoardClient;
}

function mkTask(overrides: Record<string, unknown>): Record<string, unknown> {
  const now = new Date().toISOString();
  return {
    index: 1, title: 't', description: '', status: 'Next',
    owner: 'Kade', priority: 'P2', domains: [], apiId: 100,
    created: now, updated: now, done: false,
    ...overrides,
  };
}

describe('auditClose — diff against prior snapshot', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkTmp();
    __setTestPaths({ snapshotDir: tmp });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it('newlyDone counts cards that moved from non-Done to Done between snapshots', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'WIP', title: 'in progress' }),
      mkTask({ index: 2, status: 'Next', title: 'queued' }),
    ];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
      // During session, #1 gets shipped
      mock.tasks = [
        mkTask({ index: 1, status: 'Done', title: 'in progress' }),
        mkTask({ index: 2, status: 'Next', title: 'queued' }),
      ];
      const r = await auditClose(asBoardClient(mock), 'kade');
      expect(r.newlyDone).toBe(1);
      expect(r.newCards).toBe(0);
      expect(r.retroactive).toBe(0);
    } finally { cap.restore(); }
  });

  it('newCards counts ids not present at session start', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'WIP' })];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
      // Add two new cards during session
      mock.tasks = [
        mkTask({ index: 1, status: 'WIP' }),
        mkTask({ index: 2, status: 'Next' }),
        mkTask({ index: 3, status: 'Later' }),
      ];
      const r = await auditClose(asBoardClient(mock), 'kade');
      expect(r.newCards).toBe(2);
    } finally { cap.restore(); }
  });

  it('retroactive counts cards created AND completed in same session', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'WIP' })];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
      mock.tasks = [
        mkTask({ index: 1, status: 'WIP' }),
        mkTask({ index: 99, status: 'Done', title: 'created-and-shipped' }),
      ];
      const r = await auditClose(asBoardClient(mock), 'kade');
      expect(r.retroactive).toBe(1);
      expect(r.newlyDone).toBe(1);
    } finally { cap.restore(); }
  });

  it('prints RETROACTIVE flag next to retroactive newlyDone entries', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'WIP' })];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
      mock.tasks = [
        mkTask({ index: 1, status: 'WIP' }),
        mkTask({ index: 99, status: 'Done', title: 'late-card' }),
      ];
      await auditClose(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(cap.logs.some((l) => /RETROACTIVE/.test(l))).toBe(true);
  });

  it('"Still In Progress" section lists Now cards that were Now at session start', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'Now', title: 'long-running' })];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
      // #1 is still Now, didn't ship this session
      await auditClose(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(cap.logs.some((l) => /Still In Progress/.test(l))).toBe(true);
    expect(cap.logs.some((l) => /#1.*long-running/.test(l))).toBe(true);
  });

  it('returns zeros when nothing changed between start and close', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'Next' })];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
      const r = await auditClose(asBoardClient(mock), 'kade');
      expect(r).toEqual({ newCards: 0, newlyDone: 0, retroactive: 0 });
    } finally { cap.restore(); }
  });

  it('new non-Done cards surface in "New cards created" section', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'Next' })];
    const cap = silence();
    try {
      await auditStart(asBoardClient(mock), 'kade');
      mock.tasks = [
        mkTask({ index: 1, status: 'Next' }),
        mkTask({ index: 2, status: 'Later', title: 'fresh-card' }),
      ];
      await auditClose(asBoardClient(mock), 'kade');
    } finally { cap.restore(); }
    expect(cap.logs.some((l) => /New cards created/.test(l))).toBe(true);
    expect(cap.logs.some((l) => /fresh-card/.test(l))).toBe(true);
  });
});

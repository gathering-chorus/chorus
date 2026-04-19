/**
 * runCli bulk/audit/swat tests (#2241 wave 7).
 *
 * Covers sequence-tag, bulk-move, swat, audit-start, audit-close cases —
 * the remaining big-block uncovered commands in cli.ts.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { runCli } from '../src/cli';
import {
  __setTestPaths,
  __resetTestPaths,
} from '../src/sdk';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: Map<number, BoardTask> = new Map();

  private rec(m: string, a: unknown[]) { this.calls.push({ method: m, args: a }); }

  async list() { this.rec('list', []); return Array.from(this.tasks.values()); }
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    const m = new Map<string, BoardTask[]>();
    for (const t of this.tasks.values()) {
      const s = (t as { status?: string }).status ?? 'Later';
      if (!m.has(s)) m.set(s, []);
      m.get(s)!.push(t);
    }
    return m;
  }
  async view(index: number) {
    this.rec('view', [index]);
    const t = this.tasks.get(index);
    if (!t) throw new Error(`no task ${index}`);
    return t;
  }
  async add(title: string, opts?: unknown): Promise<BoardTask> {
    this.rec('add', [title, opts]);
    const index = (this.tasks.size + 1) * 100;
    const task = { index, title, description: '', status: (opts as { status?: string })?.status ?? 'SWAT', owner: 'Kade', priority: 'P1', domains: [], apiId: index + 1 } as unknown as BoardTask;
    this.tasks.set(index, task);
    return task;
  }
  async tag(i: number, c: string, v: string) { this.rec('tag', [i, c, v]); }
  async move(i: number, s: string) { this.rec('move', [i, s]); }
  async snapshot() {
    this.rec('snapshot', []);
    return { board: this.boardName, timestamp: '2026-04-19T10:00:00Z', tasks: Array.from(this.tasks.values()) };
  }
  async mine(role: string) { this.rec('mine', [role]); return []; }
  async comment(i: number, t: string) { this.rec('comment', [i, t]); }
  async comments(_i: number) { return []; }
}

function factory(mock: MockClient) {
  return () => mock as unknown as BoardClient;
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

describe('runCli — bulk', () => {
  it('sequence-tag <id>[,<id>,...] <seq> calls tag() per id', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'sequence-tag', '1,2,3', 'quality'], factory(mock));
    } finally { cap.restore(); }
    const tagCalls = mock.calls.filter((c) => c.method === 'tag');
    expect(tagCalls).toHaveLength(3);
    expect(tagCalls.every((c) => c.args[1] === 'sequence' && c.args[2] === 'quality')).toBe(true);
  });

  it('sequence-tag without args → Usage error', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'sequence-tag', 'only-one-arg'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('bulk-move <ids> <status> moves each id', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'bulk-move', '1,2', 'Later'], factory(mock));
    } finally { cap.restore(); }
    const moves = mock.calls.filter((c) => c.method === 'move');
    expect(moves.map((c) => c.args[0]).sort()).toEqual([1, 2]);
    expect(moves.every((c) => c.args[1] === 'Later')).toBe(true);
  });

  it('bulk-move with empty id list → dies', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'bulk-move', 'not-a-number', 'Later'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });
});

describe('runCli — swat', () => {
  it('swat "description" creates a SWAT card', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'swat', 'urgent: deploy is broken'], factory(mock));
    } finally { cap.restore(); }
    const addCall = mock.calls.find((c) => c.method === 'add');
    expect(addCall).toBeDefined();
    const title = (addCall!.args[0] as string).toLowerCase();
    expect(title).toMatch(/swat|urgent/i);
  });

  it('swat without description → dies', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'swat'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });
});

describe('runCli — audit-start / audit-close', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'cards-cli-audit-'));
    __setTestPaths({ snapshotDir: tmp });
  });

  afterEach(() => {
    __resetTestPaths();
    fs.rmSync(tmp, { recursive: true, force: true });
  });

  it('audit-start <role> runs and writes snapshot', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'audit-start', 'kade'], factory(mock));
    } finally { cap.restore(); }
    const snapFile = path.join(tmp, 'board-snapshot-gathering-kade.json');
    expect(fs.existsSync(snapFile)).toBe(true);
  });

  it('audit-close <role> runs cleanly even when no snapshot exists', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'audit-close', 'kade'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.some((l) => /No start-of-session snapshot/i.test(l))).toBe(true);
  });
});

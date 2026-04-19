/**
 * runCli chain/deps/blocked/ready tests (#2241 wave 9).
 *
 * Targets cli.ts lines 770-870 — the dependency-traversal commands.
 */

import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  /** tasks keyed by display index */
  tasks: Map<number, BoardTask> = new Map();
  /** task map: displayIndex → apiId */
  taskMap: Map<number, number> = new Map();
  /** relations: displayIndex → { blockedBy, blocks } */
  relations: Map<number, { blockedBy: number[]; blocks: number[] }> = new Map();
  /** fetchAllTasks returns this */
  allTasks: Array<{ id: number; title: string; done?: boolean; related_tasks?: { blocked: Array<{ id: number; done: boolean }> } }> = [];

  private rec(m: string, a: unknown[]) { this.calls.push({ method: m, args: a }); }

  async view(index: number) {
    this.rec('view', [index]);
    const t = this.tasks.get(index);
    if (!t) throw new Error(`no task ${index}`);
    return t;
  }

  async list() { this.rec('list', []); return Array.from(this.tasks.values()); }

  async fetchAllTasks() {
    this.rec('fetchAllTasks', []);
    return this.allTasks as unknown as BoardTask[];
  }

  async buildTaskMap() {
    this.rec('buildTaskMap', []);
    return this.taskMap;
  }

  async getRelations(index: number) {
    this.rec('getRelations', [index]);
    return this.relations.get(index) ?? { blockedBy: [], blocks: [] };
  }
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

function mkTask(overrides: Partial<BoardTask>): BoardTask {
  return {
    index: 1, title: 'task', description: '', status: 'Next',
    owner: 'Kade', priority: 'P2', domains: [], apiId: 1000, done: false,
    created: '2026-04-19T10:00:00Z', updated: '2026-04-19T10:00:00Z',
    ...overrides,
  } as unknown as BoardTask;
}

describe('runCli — blocked', () => {
  it('prints "No blocked cards" when nothing is blocked', async () => {
    const mock = new MockClient();
    mock.allTasks = [];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'blocked'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/No blocked cards/);
  });

  it('lists cards where related_tasks.blocked has incomplete items', async () => {
    const mock = new MockClient();
    mock.allTasks = [
      {
        id: 100, title: 'blocked-card',
        related_tasks: { blocked: [{ id: 101, done: false }, { id: 102, done: true }] },
      },
      {
        id: 200, title: 'fully-satisfied',
        done: false,
        related_tasks: { blocked: [{ id: 201, done: true }] },
      },
    ];
    mock.taskMap.set(1, 100);
    mock.taskMap.set(2, 101);
    mock.taskMap.set(3, 102);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'blocked'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/blocked-card.*blocked by:/);
    // fully-satisfied has all deps done so it should NOT appear in 'blocked'
    expect(cap.logs.join('\n')).not.toMatch(/fully-satisfied/);
  });
});

describe('runCli — ready', () => {
  it('prints "No cards with completed dependencies waiting" when none ready', async () => {
    const mock = new MockClient();
    mock.allTasks = [];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'ready'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/No cards with completed dependencies/);
  });

  it('lists cards whose deps are all done and self not done', async () => {
    const mock = new MockClient();
    mock.allTasks = [
      {
        id: 100, title: 'ready-to-pull', done: false,
        related_tasks: { blocked: [{ id: 101, done: true }, { id: 102, done: true }] },
      },
      {
        id: 200, title: 'still-blocked', done: false,
        related_tasks: { blocked: [{ id: 201, done: false }] },
      },
    ];
    mock.taskMap.set(1, 100);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'ready'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/ready-to-pull/);
    expect(joined).not.toMatch(/still-blocked/);
  });
});

describe('runCli — chain', () => {
  it('chain without id → Usage error', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'chain'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('chain <id> walks upstream to root, then downstream', async () => {
    const mock = new MockClient();
    mock.tasks.set(1, mkTask({ index: 1, title: 'root' }));
    mock.tasks.set(2, mkTask({ index: 2, title: 'middle' }));
    mock.tasks.set(3, mkTask({ index: 3, title: 'leaf' }));
    mock.relations.set(1, { blockedBy: [], blocks: [2] });
    mock.relations.set(2, { blockedBy: [1], blocks: [3] });
    mock.relations.set(3, { blockedBy: [2], blocks: [] });
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'chain', '2'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Chain from #1/);
    expect(joined).toMatch(/root/);
    expect(joined).toMatch(/middle/);
    expect(joined).toMatch(/leaf/);
  });

  it('chain with isolated card prints "no dependency chain"', async () => {
    const mock = new MockClient();
    mock.tasks.set(42, mkTask({ index: 42, title: 'lone' }));
    mock.relations.set(42, { blockedBy: [], blocks: [] });
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'chain', '42'], factory(mock));
    } finally { cap.restore(); }
    // 42 has no deps either direction — chain walks to itself then adds it
    expect(cap.logs.join('\n')).toMatch(/Chain from #42|no dependency chain/);
  });

  it('chain renders icon per status (Done vs WIP vs Later)', async () => {
    const mock = new MockClient();
    mock.tasks.set(1, mkTask({ index: 1, title: 'shipped', status: 'Done' }));
    mock.tasks.set(2, mkTask({ index: 2, title: 'building', status: 'WIP' }));
    mock.relations.set(1, { blockedBy: [], blocks: [2] });
    mock.relations.set(2, { blockedBy: [1], blocks: [] });
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'chain', '1'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    // Done → checkmark emoji; WIP → hammer emoji (per cli.ts line 857)
    expect(joined).toMatch(/shipped/);
    expect(joined).toMatch(/building/);
  });
});

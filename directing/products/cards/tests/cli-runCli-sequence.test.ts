/**
 * runCli sequence + domain-remove + label tests (#2241 wave 10).
 * Targets the remaining cli.ts gaps: cmdDomain remove, cmdSequence inner.
 */

import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: BoardTask[] = [];
  deleteLabelShouldThrow: Error | null = null;

  private rec(m: string, a: unknown[]) { this.calls.push({ method: m, args: a }); }

  async list() { this.rec('list', []); return this.tasks; }
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    const m = new Map<string, BoardTask[]>();
    for (const t of this.tasks) {
      const s = (t as { status?: string }).status ?? 'Later';
      if (!m.has(s)) m.set(s, []);
      m.get(s)!.push(t);
    }
    return m;
  }
  async mine(role: string) { this.rec('mine', [role]); return []; }
  async now(role: string) { this.rec('now', [role]); return []; }
  async listLabels() {
    this.rec('listLabels', []);
    return [{ id: 42, title: 'domain:temporary' }, { id: 1, title: 'P1' }];
  }
  async createLabel(title: string) { this.rec('createLabel', [title]); return { id: 99, title }; }
  async deleteLabel(id: number) {
    this.rec('deleteLabel', [id]);
    if (this.deleteLabelShouldThrow) throw this.deleteLabelShouldThrow;
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

describe('runCli — domain remove', () => {
  it('remove <name> deletes label when no cards use it', async () => {
    const mock = new MockClient();
    mock.tasks = [];
    const cap = silence();
    try {
      // 'temporary' isn't in LABELS.domain, so it'll fall through to
      // listLabels → find match → deleteLabel path.
      await runCli(['node', 'cards', 'domain', 'remove', 'temporary'], factory(mock));
    } finally { cap.restore(); }
    const del = mock.calls.find((c) => c.method === 'deleteLabel');
    expect(del).toBeDefined();
    expect(del!.args[0]).toBe(42);
  });

  it('remove refuses to delete when cards still use the domain', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, title: 'uses-it', status: 'Next', domains: ['domain:temporary'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'domain', 'remove', 'temporary'], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'deleteLabel')).toBeUndefined();
    expect(cap.logs.join('\n')).toMatch(/card\(s\) use domain:temporary/);
  });

  it('remove <name> dies when name missing', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'domain', 'remove'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('remove tolerates 401 on deleteLabel with friendly message', async () => {
    const mock = new MockClient();
    mock.tasks = [];
    mock.deleteLabelShouldThrow = new Error('401 unauthorized');
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'domain', 'remove', 'temporary'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/Cannot delete label/);
  });

  it('remove not-found domain dies', async () => {
    const mock = new MockClient();
    mock.tasks = [];
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'domain', 'remove', 'does-not-exist'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });
});

describe('runCli — sequence detail', () => {
  it('sequence <name> lists active cards grouped by status', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'WIP', title: 'q-wip', domains: ['sequence:quality'] }),
      mkTask({ index: 2, status: 'Next', title: 'q-next', domains: ['sequence:quality'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'sequence', 'quality'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/sequence:quality \(2 active\)/);
    expect(joined).toMatch(/q-wip/);
    expect(joined).toMatch(/q-next/);
  });

  it('sequence <name> with no matching cards prints "No active"', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'Next', domains: ['sequence:ops'] })];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'sequence', 'quality'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/No active cards tagged sequence:quality/);
  });

  it('sequence <name> surfaces Done cards below active', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', title: 'active', domains: ['sequence:quality'] }),
      mkTask({ index: 2, status: 'Done', title: 'shipped', domains: ['sequence:quality'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'sequence', 'quality'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Done \(1\)/);
    expect(joined).toMatch(/shipped/);
  });
});

/**
 * runCli integration tests (#2241).
 *
 * Exercises the dispatcher with a mock BoardClient so jest covers the many
 * command branches in cli.ts without real HTTP, real files, or real
 * processes. Each test passes `clientFactory` to runCli and asserts either
 * a stdout line the user would see or a method call on the mock.
 *
 * Tests describe what a role types at the terminal, not internal helpers.
 */

import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

/**
 * Minimal BoardClient test double. Records calls for assertions; returns
 * canned data for read methods.
 */
class MockClient {
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: BoardTask[] = [];
  // #2707 — track which indexes have had done() called so view() can return
  // status='Done' for them, modeling a working board. Without this, doneCard's
  // verify-after-move catches the mock as a silent-failure.
  doneCalled = new Set<number>();

  record(method: string, args: unknown[]): void {
    this.calls.push({ method, args });
  }

  async list(): Promise<BoardTask[]> {
    this.record('list', []);
    return this.tasks;
  }

  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    this.record('listGrouped', []);
    const map = new Map<string, BoardTask[]>();
    for (const t of this.tasks) {
      const s = (t as { status?: string }).status ?? 'Later';
      if (!map.has(s)) map.set(s, []);
      map.get(s)!.push(t);
    }
    return map;
  }

  async mine(role: string): Promise<BoardTask[]> {
    this.record('mine', [role]);
    return this.tasks.filter((t) => (t.owner ?? '').toLowerCase() === role.toLowerCase());
  }

  async now(role: string): Promise<BoardTask[]> {
    this.record('now', [role]);
    return [];
  }

  async view(index: number): Promise<BoardTask> {
    this.record('view', [index]);
    const found = this.tasks.find((t) => t.index === index);
    if (!found) throw new Error(`Task ${index} not found`);
    if (this.doneCalled.has(index)) {
      return { ...found, status: 'Done', done: true } as unknown as BoardTask;
    }
    return found;
  }

  async fetchBuckets(): Promise<Array<{ id: number; title: string; limit: number; taskCount: number }>> {
    this.record('fetchBuckets', []);
    return [];
  }

  async fetchBucketsWithLimits(): Promise<Array<{ id: number; title: string; limit: number; taskCount: number }>> {
    this.record('fetchBucketsWithLimits', []);
    return [];
  }

  async add(title: string, opts?: unknown): Promise<BoardTask> {
    this.record('add', [title, opts]);
    const created: BoardTask = {
      index: 9999, title, description: '', status: 'Later', owner: 'Kade', priority: 'P2',
      domains: [], product: undefined, apiId: 10000,
    } as unknown as BoardTask;
    return created;
  }

  async move(index: number, status: string): Promise<void> {
    this.record('move', [index, status]);
  }

  async done(index: number): Promise<void> {
    this.record('done', [index]);
    this.doneCalled.add(index);
  }

  async comment(index: number, text: string): Promise<void> {
    this.record('comment', [index, text]);
  }

  async comments(_index: number): Promise<Array<{ author: string; text: string }>> {
    this.record('comments', [_index]);
    return [];
  }

  async update(index: number, fields: unknown): Promise<void> {
    this.record('update', [index, fields]);
  }

  async tag(index: number, category: string, value: string): Promise<void> {
    this.record('tag', [index, category, value]);
  }

  async listLabels(): Promise<Array<{ id: number; title: string }>> {
    this.record('listLabels', []);
    return [{ id: 1, title: 'P1' }];
  }

  async createLabel(title: string): Promise<{ id: number; title: string }> {
    this.record('createLabel', [title]);
    return { id: 999, title };
  }
}

function factory(mock: MockClient): (cfg: unknown) => BoardClient {
  return () => mock as unknown as BoardClient;
}

function captureConsole(): { logs: string[]; errs: string[]; restore: () => void } {
  const logs: string[] = [];
  const errs: string[] = [];
  const origLog = console.log;
  const origErr = console.error;
  console.log = (...a) => { logs.push(a.join(' ')); };
  console.error = (...a) => { errs.push(a.join(' ')); };
  return {
    logs, errs,
    restore: () => { console.log = origLog; console.error = origErr; },
  };
}

describe('runCli — dispatch', () => {
  it('no args → prints usage, does not throw', async () => {
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards']);
    } finally {
      cap.restore();
    }
    expect(cap.logs.join('\n')).toMatch(/Usage/i);
  });

  it('help command → prints usage', async () => {
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'help'], factory(new MockClient()));
    } finally {
      cap.restore();
    }
    expect(cap.logs.join('\n')).toMatch(/Usage/i);
  });

  it('--help flag → prints usage', async () => {
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', '--help'], factory(new MockClient()));
    } finally {
      cap.restore();
    }
    expect(cap.logs.join('\n')).toMatch(/Usage/i);
  });

  it('list command → invokes client.listGrouped', async () => {
    const mock = new MockClient();
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'list'], factory(mock));
    } finally {
      cap.restore();
    }
    expect(mock.calls.some((c) => c.method === 'listGrouped')).toBe(true);
  });

  it('mine <role> → invokes client.mine with the role', async () => {
    const mock = new MockClient();
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'mine', 'kade'], factory(mock));
    } finally {
      cap.restore();
    }
    const mineCall = mock.calls.find((c) => c.method === 'mine');
    expect(mineCall).toBeDefined();
    expect(mineCall!.args[0]).toBe('kade');
  });

  it('view <id> → invokes client.view with parsed index', async () => {
    const mock = new MockClient();
    mock.tasks = [{
      index: 42, title: 't', description: '', status: 'WIP', owner: 'Kade', priority: 'P1',
      domains: [], product: undefined, apiId: 100,
    } as unknown as BoardTask];
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'view', '42'], factory(mock));
    } finally {
      cap.restore();
    }
    expect(mock.calls.find((c) => c.method === 'view')?.args[0]).toBe(42);
  });

  it('move <id> <status> to non-gated status → invokes client.move', async () => {
    const mock = new MockClient();
    mock.tasks = [{
      index: 42, title: 't', description: '', status: 'Later', owner: 'Kade', priority: 'P1',
      domains: [], product: undefined, apiId: 100,
    } as unknown as BoardTask];
    const cap = captureConsole();
    try {
      // Move to Later (no WIP gate checks) so test focuses on dispatch,
      // not AC/Experience enforcement.
      await runCli(['node', 'cards', 'move', '42', 'Later'], factory(mock));
    } finally {
      cap.restore();
    }
    const call = mock.calls.find((c) => c.method === 'move');
    expect(call).toBeDefined();
    expect(call!.args).toEqual([42, 'Later']);
  });

  it('done <id> → invokes client.done', async () => {
    const mock = new MockClient();
    mock.tasks = [{
      index: 42, title: 't', description: '', status: 'WIP', owner: 'Kade', priority: 'P1',
      domains: [], product: undefined, apiId: 100,
    } as unknown as BoardTask];
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'done', '42'], factory(mock));
    } finally {
      cap.restore();
    }
    expect(mock.calls.find((c) => c.method === 'done')?.args[0]).toBe(42);
  });

  it('comment <id> <text> → invokes client.comment with text', async () => {
    const mock = new MockClient();
    mock.tasks = [{
      index: 42, title: 't', description: '', status: 'WIP', owner: 'Kade', priority: 'P1',
      domains: [], product: undefined, apiId: 100,
    } as unknown as BoardTask];
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'comment', '42', 'hello world'], factory(mock));
    } finally {
      cap.restore();
    }
    const call = mock.calls.find((c) => c.method === 'comment');
    expect(call).toBeDefined();
    expect(call!.args[0]).toBe(42);
    expect(call!.args[1]).toBe('hello world');
  });

  it('fields command → prints field reference without hitting client', async () => {
    const mock = new MockClient();
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'fields'], factory(mock));
    } finally {
      cap.restore();
    }
    // fields command prints config; no BoardClient method invoked
    expect(cap.logs.length).toBeGreaterThan(0);
  });

  it('buckets command → invokes client.fetchBucketsWithLimits', async () => {
    const mock = new MockClient();
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'buckets'], factory(mock));
    } finally {
      cap.restore();
    }
    expect(mock.calls.some((c) =>
      c.method === 'fetchBuckets' || c.method === 'fetchBucketsWithLimits',
    )).toBe(true);
  });

  it('label list → invokes client.listLabels', async () => {
    const mock = new MockClient();
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'label', 'list'], factory(mock));
    } finally {
      cap.restore();
    }
    expect(mock.calls.find((c) => c.method === 'listLabels')).toBeDefined();
  });

  it('label create <title> → invokes client.createLabel', async () => {
    const mock = new MockClient();
    const cap = captureConsole();
    try {
      await runCli(['node', 'cards', 'label', 'create', 'new-label'], factory(mock));
    } finally {
      cap.restore();
    }
    expect(mock.calls.find((c) => c.method === 'createLabel')?.args[0]).toBe('new-label');
  });

  it('unknown command → exits with error (die called)', async () => {
    const mock = new MockClient();
    const cap = captureConsole();
    const origExit = process.exit;
    let exitCode: number | undefined;
    process.exit = ((code?: number) => {
      exitCode = code;
      throw new Error(`process.exit(${code})`);
    }) as typeof process.exit;
    try {
      await runCli(['node', 'cards', 'bogus-command'], factory(mock));
    } catch {
      // die() triggers process.exit; our mock throws so the rest of the
      // function doesn't run. We just care that exit was signalled.
    } finally {
      process.exit = origExit;
      cap.restore();
    }
    expect(exitCode).toBe(1);
  });
});

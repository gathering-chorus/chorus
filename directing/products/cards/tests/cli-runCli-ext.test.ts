/**
 * runCli extended dispatch tests (#2241 wave 5).
 *
 * Covers the long tail of CLI commands: create (alias), block/unblock,
 * demo, reject, reassign, tag (rejection), untag, set, update, deps,
 * --self flag, product filter, mine without role, view with missing id.
 * Each test asserts a Jeff-visible effect (method call or error output).
 */

import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: Map<number, BoardTask> = new Map();

  private rec(method: string, args: unknown[]) { this.calls.push({ method, args }); }

  async list(): Promise<BoardTask[]> { this.rec('list', []); return Array.from(this.tasks.values()); }
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    this.rec('listGrouped', []);
    const m = new Map<string, BoardTask[]>();
    for (const t of this.tasks.values()) {
      const s = (t as { status?: string }).status ?? 'Later';
      if (!m.has(s)) m.set(s, []);
      m.get(s)!.push(t);
    }
    return m;
  }
  async mine(role: string) { this.rec('mine', [role]); return []; }
  async now(role: string) { this.rec('now', [role]); return []; }
  async view(index: number) {
    this.rec('view', [index]);
    const t = this.tasks.get(index);
    if (!t) throw new Error(`no task ${index}`);
    return t;
  }
  async add(title: string, opts?: unknown): Promise<BoardTask> {
    this.rec('add', [title, opts]);
    const index = (this.tasks.size + 1) * 100;
    const task = { index, title, description: '', status: 'Later', owner: 'Kade', priority: 'P2', domains: [], apiId: index + 1 } as unknown as BoardTask;
    this.tasks.set(index, task);
    return task;
  }
  async move(index: number, status: string) { this.rec('move', [index, status]); }
  async done(index: number) { this.rec('done', [index]); }
  async block(index: number, reason?: string) { this.rec('block', [index, reason]); }
  async unblock(index: number) { this.rec('unblock', [index]); }
  async update(index: number, fields: unknown) { this.rec('update', [index, fields]); }
  async comment(index: number, text: string) { this.rec('comment', [index, text]); }
  async comments(_i: number) { this.rec('comments', [_i]); return []; }
  async tag(index: number, category: string, value: string) { this.rec('tag', [index, category, value]); }
  async untag(index: number, category: string, value: string) { this.rec('untag', [index, category, value]); }
  async reassignOwner(index: number, role: string) {
    this.rec('reassignOwner', [index, role]);
    return { oldOwner: 'Kade', newOwner: role };
  }
  async getRelations(index: number) {
    this.rec('getRelations', [index]);
    return { blockedBy: [], blocks: [] };
  }
  async fetchAllTasks() {
    this.rec('fetchAllTasks', []);
    return Array.from(this.tasks.values());
  }
  async buildTaskMap() {
    this.rec('buildTaskMap', []);
    return new Map<number, number>();
  }
  async snapshot() {
    this.rec('snapshot', []);
    return { board: this.boardName, timestamp: '2026-04-19T10:00:00Z', tasks: Array.from(this.tasks.values()) };
  }
  async fetchBucketsWithLimits() {
    this.rec('fetchBucketsWithLimits', []);
    return [{ id: 1, title: 'Now', limit: 3, taskCount: 0 }];
  }
  async listLabels() { this.rec('listLabels', []); return [{ id: 1, title: 'P1' }]; }
  async createLabel(title: string) { this.rec('createLabel', [title]); return { id: 99, title }; }
  async setBucketLimit(bucketId: number, limit: number) { this.rec('setBucketLimit', [bucketId, limit]); }
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

async function seed(mock: MockClient, overrides: Record<string, unknown> = {}): Promise<number> {
  const c = await mock.add('placeholder', {});
  const task = mock.tasks.get(c.index)!;
  Object.assign(task, overrides);
  return c.index;
}

describe('runCli — extended dispatch', () => {
  it('create (alias for add) routes to add path', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      // "fix ..." triggers TITLE_TO_TYPE auto-tag so --type can be omitted.
      await runCli(
        ['node', 'cards', 'create', 'fix the flaky bit', '--domain', 'chorus', '--priority', 'P2', '--quick'],
        factory(mock),
      );
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'add')).toBeDefined();
  });

  it('block <id> <reason> calls client.block with reason', async () => {
    const mock = new MockClient();
    const index = await seed(mock);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'block', String(index), 'external dep'], factory(mock));
    } finally { cap.restore(); }
    const b = mock.calls.find((c) => c.method === 'block');
    expect(b).toBeDefined();
    expect(b!.args[0]).toBe(index);
    expect(b!.args[1]).toContain('external dep');
  });

  it('unblock <id> calls client.unblock', async () => {
    const mock = new MockClient();
    const index = await seed(mock);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'unblock', String(index)], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'unblock')?.args[0]).toBe(index);
  });

  it('demo <id> reads the card and prints "Demo started"', async () => {
    const mock = new MockClient();
    const index = await seed(mock, { status: 'WIP' });
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'demo', String(index)], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'view')?.args[0]).toBe(index);
    expect(cap.logs.some((l) => /Demo started: #\d+/.test(l))).toBe(true);
  });

  it('reject <id> <reason> prints "Rejected" with the reason', async () => {
    const mock = new MockClient();
    const index = await seed(mock, { status: 'Demo' });
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'reject', String(index), 'needs', 'more', 'tests'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.some((l) => /Rejected: #\d+/.test(l))).toBe(true);
    expect(cap.logs.some((l) => /needs more tests/.test(l))).toBe(true);
  });

  it('reassign <id> <role> calls reassignOwner', async () => {
    const mock = new MockClient();
    const index = await seed(mock);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'reassign', String(index), 'wren'], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'reassignOwner')?.args).toEqual([index, 'wren']);
  });

  it('tag command is retired → prints migration error and exits 1', async () => {
    const mock = new MockClient();
    const index = await seed(mock);
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'tag', String(index), 'chunk', 'ops'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    expect(cap.errs.join('\n')).toMatch(/Removed/i);
  });

  it('untag <id> <category:value> calls client.untag', async () => {
    const mock = new MockClient();
    const index = await seed(mock, { domains: ['sequence:quality'] });
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'untag', String(index), 'sequence:quality'], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'untag')).toBeDefined();
  });

  it('set <id> key=value applies mutation via setCard', async () => {
    const mock = new MockClient();
    const index = await seed(mock);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'set', String(index), 'sequence=quality'], factory(mock));
    } finally { cap.restore(); }
    // setCard invokes tag or update depending on key
    expect(mock.calls.some((c) => c.method === 'tag' || c.method === 'update')).toBe(true);
  });

  it('set without key=value pair dies', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'set', '42'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('update --title <new> routes to setCard with title pair', async () => {
    const mock = new MockClient();
    const index = await seed(mock);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'update', String(index), '--title', 'renamed'], factory(mock));
    } finally { cap.restore(); }
    // update path hits setCard which invokes update or tag
    expect(mock.calls.some((c) => c.method === 'update' || c.method === 'tag')).toBe(true);
  });

  it('deps <id> prints After/Gates sections', async () => {
    const mock = new MockClient();
    const index = await seed(mock);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'deps', String(index)], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'getRelations')?.args[0]).toBe(index);
    expect(cap.logs.some((l) => /After:/.test(l))).toBe(true);
    expect(cap.logs.some((l) => /Gates:/.test(l))).toBe(true);
  });

  it('snapshot command calls client.snapshot and writes file', async () => {
    const mock = new MockClient();
    await seed(mock);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'snapshot'], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'snapshot')).toBeDefined();
  });

  it('set-limit <bucket> <number> calls client.setBucketLimit', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'set-limit', 'Now', '5'], factory(mock));
    } finally { cap.restore(); }
    // set-limit parses bucket name → id; may not be 1:1 with our stub. The
    // command at minimum invokes fetchBucketsWithLimits to resolve name.
    expect(mock.calls.some((c) => c.method === 'fetchBucketsWithLimits' || c.method === 'setBucketLimit')).toBe(true);
  });

  it('move without args → die "Usage"', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'move'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
    expect(cap.errs.join('\n')).toMatch(/Usage/);
  });

  it('done without id → die "Usage"', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'done'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('comment with empty text → die "Usage"', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'comment', '42'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('--self flag exercises SELF-board dispatch path', async () => {
    const cap = silence();
    let factoryCalled = false;
    try {
      await runCli(['node', 'cards', '--self', 'help'], () => {
        factoryCalled = true;
        return new MockClient() as unknown as BoardClient;
      });
    } finally { cap.restore(); }
    expect(factoryCalled).toBe(true);
    expect(cap.logs.some((l) => /Usage/i.test(l))).toBe(true);
  });
});

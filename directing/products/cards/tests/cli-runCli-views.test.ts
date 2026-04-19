/**
 * runCli view/list/chunk/domain/sequence tests (#2241 wave 6).
 *
 * Targets cmdView, cmdChunk, cmdDomain, cmdSequence, cmdFilter — the
 * display commands that form the long tail of uncovered cli.ts lines.
 */

import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: Map<number, BoardTask> = new Map();
  comments_by_id = new Map<number, Array<{ author: string; text: string }>>();

  private rec(m: string, a: unknown[]) { this.calls.push({ method: m, args: a }); }

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
  async view(index: number) {
    this.rec('view', [index]);
    const t = this.tasks.get(index);
    if (!t) throw new Error(`no task ${index}`);
    return t;
  }
  async mine(role: string) { this.rec('mine', [role]); return []; }
  async now(role: string) { this.rec('now', [role]); return []; }
  async comments(index: number) {
    this.rec('comments', [index]);
    return this.comments_by_id.get(index) ?? [];
  }
  async listLabels() {
    this.rec('listLabels', []);
    return [{ id: 1, title: 'P1' }, { id: 20, title: 'domain:photos' }];
  }
  async createLabel(title: string) { this.rec('createLabel', [title]); return { id: 99, title }; }
  async deleteLabel(id: number) { this.rec('deleteLabel', [id]); }
  async fetchBucketsWithLimits() {
    this.rec('fetchBucketsWithLimits', []);
    return [
      { id: 10, title: 'Now', limit: 3, taskCount: 2 },
      { id: 11, title: 'WIP', limit: 3, taskCount: 1 },
      { id: 12, title: 'Next', limit: 0, taskCount: 5 },
    ];
  }

  seed(index: number, overrides: Partial<BoardTask> = {}) {
    const base: BoardTask = {
      index, title: `card-${index}`, description: '',
      status: 'Next', owner: 'Kade', priority: 'P2',
      domains: [], apiId: index + 1000,
      created: '2026-04-19T10:00:00Z', updated: '2026-04-19T10:00:00Z', done: false,
    } as unknown as BoardTask;
    const merged = { ...base, ...overrides } as BoardTask;
    this.tasks.set(index, merged);
    return merged;
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

describe('runCli — view', () => {
  it('view <id> prints card detail', async () => {
    const mock = new MockClient();
    mock.seed(42, { title: 'detail card', owner: 'Kade', priority: 'P1', description: '## Experience\nJeff sees', domains: ['domain:chorus'] } as Partial<BoardTask>);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'view', '42'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/#42.*detail card/);
    expect(joined).toMatch(/Status:/);
    expect(joined).toMatch(/Owner:\s+Kade/);
    expect(joined).toMatch(/Priority:\s+P1/);
  });

  it('view <id> --json outputs machine-readable JSON', async () => {
    const mock = new MockClient();
    mock.seed(42, { title: 'json card', owner: 'Kade', priority: 'P1' } as Partial<BoardTask>);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'view', '42', '--json'], factory(mock));
    } finally { cap.restore(); }
    // first log line is JSON
    const first = cap.logs[0];
    expect(() => JSON.parse(first)).not.toThrow();
    const parsed = JSON.parse(first);
    expect(parsed.index).toBe(42);
    expect(parsed.title).toBe('json card');
  });

  it('view <id> with comments renders them indented', async () => {
    const mock = new MockClient();
    mock.seed(42, {});
    mock.comments_by_id.set(42, [
      { author: 'kade', text: 'first comment' },
      { author: 'wren', text: 'second comment' },
    ]);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'view', '42'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Comments \(2\)/);
    expect(joined).toMatch(/\[kade\]/);
    expect(joined).toMatch(/\[wren\]/);
  });
});

describe('runCli — chunk', () => {
  it('chunk (no arg) lists known chunks', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'chunk'], factory(mock));
    } finally { cap.restore(); }
    // cmdChunk prints known chunks when no arg; just check it ran and wrote
    expect(cap.logs.length).toBeGreaterThan(0);
  });

  it('chunk <name> filters active cards by chunk label', async () => {
    const mock = new MockClient();
    mock.seed(1, { status: 'Next', domains: ['chunk:ops', 'domain:chorus'] } as Partial<BoardTask>);
    mock.seed(2, { status: 'Next', domains: ['chunk:app'] } as Partial<BoardTask>);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'chunk', 'ops'], factory(mock));
    } finally { cap.restore(); }
    // Output should mention #1 but not #2
    const joined = cap.logs.join('\n');
    // Output shape is `  <index-padded>  <title>`, no '#' prefix.
    expect(joined).toMatch(/card-1/);
    expect(joined).not.toMatch(/card-2/);
  });
});

describe('runCli — domain', () => {
  it('domain (no arg) lists known domains', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'domain'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.length).toBeGreaterThan(0);
  });

  it('domain add <name> creates a label via client', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      // pick a name not in LABELS.domain to hit the create path
      await runCli(['node', 'cards', 'domain', 'add', 'brand-new-domain'], factory(mock));
    } finally { cap.restore(); }
    const cl = mock.calls.find((c) => c.method === 'createLabel');
    expect(cl?.args[0]).toMatch(/^domain:/);
  });
});

describe('runCli — sequence', () => {
  it('sequence (no arg) lists known sequences', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'sequence'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.length).toBeGreaterThan(0);
  });

  it('sequence-tag <ids> <seq> bulk-tags via bulkSequenceTag', async () => {
    const mock = new MockClient();
    mock.seed(1, {});
    mock.seed(2, {});
    mock.seed(3, {});
    const tag_calls: unknown[] = [];
    const tagOrig = MockClient.prototype.constructor;
    // Add tag() to the mock
    (mock as unknown as { tag: (i: number, c: string, v: string) => Promise<void> }).tag =
      async (i: number, c: string, v: string) => { tag_calls.push([i, c, v]); };
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'sequence-tag', '1', '2', '3', 'quality'], factory(mock));
    } finally { cap.restore(); }
    void tagOrig;
    // Expect 3 tag calls with sequence:quality
    expect(tag_calls).toHaveLength(3);
    expect(tag_calls.every((a) => Array.isArray(a) && a[1] === 'sequence' && a[2] === 'quality')).toBe(true);
  });
});

describe('runCli — buckets + fields', () => {
  it('buckets prints bucket list with limits and counts', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'buckets'], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'fetchBucketsWithLimits')).toBeDefined();
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Now/);
    expect(joined).toMatch(/Next/);
  });

  it('fields prints field reference without client call', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'fields'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.length).toBeGreaterThan(0);
  });
});

describe('runCli — mine and now', () => {
  it('mine (no role arg) defaults to detected role', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'mine'], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'mine')).toBeDefined();
  });

  it('now <role> invokes client.now', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'now', 'kade'], factory(mock));
    } finally { cap.restore(); }
    expect(mock.calls.find((c) => c.method === 'now')?.args[0]).toBe('kade');
  });

  it('now <role> with tasks prints per-task lines', async () => {
    const mock = new MockClient();
    // Override now to return a populated list for this role
    (mock as unknown as { now: (r: string) => Promise<unknown[]> }).now =
      async (_r: string) => [
        { index: 42, title: 'active-now', priority: 'P1' },
        { index: 43, title: 'also-now', priority: '' },
      ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'now', 'kade'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Kade — Now \(2\)/);
    expect(joined).toMatch(/active-now/);
    expect(joined).toMatch(/also-now/);
  });
});

describe('runCli — set-limit error paths', () => {
  it('set-limit <bucket> with non-numeric limit dies', async () => {
    const mock = new MockClient();
    const origExit = process.exit;
    const calls: number[] = [];
    process.exit = ((code?: number) => {
      calls.push(code ?? 0);
      throw new Error(`exit(${code})`);
    }) as typeof process.exit;
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'set-limit', 'now', 'abc'], factory(mock)).catch(() => {});
    } finally {
      process.exit = origExit;
      cap.restore();
    }
    expect(calls).toEqual([1]);
  });

  it('set-limit with unknown bucket name dies', async () => {
    const mock = new MockClient();
    const origExit = process.exit;
    const calls: number[] = [];
    process.exit = ((code?: number) => {
      calls.push(code ?? 0);
      throw new Error(`exit(${code})`);
    }) as typeof process.exit;
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'set-limit', 'not-a-bucket', '5'], factory(mock)).catch(() => {});
    } finally {
      process.exit = origExit;
      cap.restore();
    }
    expect(calls).toEqual([1]);
  });
});

/**
 * runCli edge-branch tests (#2241 wave 11).
 *
 * Targets remaining uncovered cli.ts branches: view without id, mine with
 * no tasks, view with domain-context file found, cmdChunk fallback when
 * chunk has no context doc, audit-start without role, label subcommands.
 */

import * as fs from 'fs';
import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: BoardTask[] = [];
  byIndex: Map<number, BoardTask> = new Map();
  comments_by_id: Map<number, Array<{ author: string; text: string }>> = new Map();

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
  async view(index: number) {
    this.rec('view', [index]);
    const t = this.byIndex.get(index);
    if (!t) throw new Error(`no task ${index}`);
    return t;
  }
  async mine(role: string) { this.rec('mine', [role]); return this.tasks.filter((t) => (t.owner ?? '').toLowerCase() === role.toLowerCase()); }
  async now(role: string) { this.rec('now', [role]); return []; }
  async comments(index: number) { this.rec('comments', [index]); return this.comments_by_id.get(index) ?? []; }
  async listLabels() { this.rec('listLabels', []); return [{ id: 1, title: 'P1' }]; }
  async createLabel(title: string) { this.rec('createLabel', [title]); return { id: 99, title }; }
}

function factory(mock: MockClient) { return () => mock as unknown as BoardClient; }

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

describe('runCli — edges', () => {
  it('view without id → Usage', async () => {
    const mock = new MockClient();
    const cap = silence();
    const exit = interceptExit();
    try {
      await runCli(['node', 'cards', 'view'], factory(mock)).catch(() => {});
    } finally {
      exit.restore();
      cap.restore();
    }
    expect(exit.calls).toEqual([1]);
  });

  it('mine <role> with no matching tasks → "No … items assigned"', async () => {
    const mock = new MockClient();
    // mine(role) returns empty by default
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'mine', 'wren'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/No Gathering items assigned to wren/);
  });

  it('mine <role> with tasks prints them grouped', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'WIP', title: 'doing', owner: 'Kade', priority: 'P1' }),
      mkTask({ index: 2, status: 'Next', title: 'queued', owner: 'Kade', priority: 'P2' }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'mine', 'kade'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Kade Gathering items/);
    expect(joined).toMatch(/doing/);
    expect(joined).toMatch(/queued/);
  });

  it('audit-start with explicit role writes snapshot file', async () => {
    const os = await import('os');
    const tmpPath = await import('path');
    const tmp = fs.mkdtempSync(tmpPath.join(os.tmpdir(), 'cli-edge-audit-'));
    const { __setTestPaths, __resetTestPaths } = await import('../src/sdk');
    __setTestPaths({ snapshotDir: tmp });
    class Extended extends MockClient {
      async snapshot() {
        (this as unknown as { calls: Array<{ method: string; args: unknown[] }> }).calls.push({ method: 'snapshot', args: [] });
        return { board: 'gathering', timestamp: '2026-04-19T10:00:00Z', tasks: [] };
      }
    }
    const mock = new Extended();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'audit-start', 'kade'], factory(mock));
    } finally {
      cap.restore();
      __resetTestPaths();
      fs.rmSync(tmp, { recursive: true, force: true });
    }
    expect(mock.calls.find((c) => c.method === 'snapshot')).toBeDefined();
  });

  it('view <id> --verbose passes through to comment renderer', async () => {
    const mock = new MockClient();
    const task = mkTask({ index: 42, title: 'verbose card' });
    mock.byIndex.set(42, task);
    mock.comments_by_id.set(42, [{ author: 'kade', text: '**Blast Radius** — 30 files\nline1\nline2' }]);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'view', '42', '--verbose'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    // Verbose renders full comment body including line2
    expect(joined).toMatch(/line2/);
  });

  it('view <id> default (no --verbose) truncates auto-generated comment', async () => {
    const mock = new MockClient();
    const task = mkTask({ index: 42, title: 'default-view' });
    mock.byIndex.set(42, task);
    mock.comments_by_id.set(42, [{ author: 'kade', text: '**Blast Radius** — 30 files\nline1\nline2\nline3' }]);
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'view', '42'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    // Default mode shows first line + "use --verbose" hint
    expect(joined).toMatch(/--verbose/);
    expect(joined).not.toMatch(/line3/);
  });

  it('chunk <unknown> falls through to chunk-list view', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', title: 'one', domains: ['chunk:ops'] }),
    ];
    const cap = silence();
    try {
      // "weird-chunk" isn't in validChunks so we land on the list-view path.
      await runCli(['node', 'cards', 'chunk', 'weird-chunk'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Chunks:/);
    expect(joined).toMatch(/ops/);
  });

  it('domain add <existing> is a no-op with "already exists" message', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      // "chorus" is in LABELS.domain already
      await runCli(['node', 'cards', 'domain', 'add', 'chorus'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/already exists/);
    expect(mock.calls.find((c) => c.method === 'createLabel')).toBeUndefined();
  });
});

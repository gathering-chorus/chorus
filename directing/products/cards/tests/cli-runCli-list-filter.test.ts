/**
 * runCli list + filter tests (#2241 wave 8).
 *
 * cmdList and cmdFilter are the biggest remaining uncovered blocks in
 * cli.ts (lines 126-191). Both exercise tags, domain rendering,
 * product-filter behavior.
 */

import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: BoardTask[] = [];

  private rec(m: string, a: unknown[]) { this.calls.push({ method: m, args: a }); }

  async list() { this.rec('list', []); return this.tasks; }
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    this.rec('listGrouped', []);
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

function mkTask(overrides: Partial<BoardTask>): BoardTask {
  return {
    index: 1, title: 'task', description: '', status: 'Next',
    owner: 'Kade', priority: 'P2', domains: [], apiId: 1000, done: false,
    created: '2026-04-19T10:00:00Z', updated: '2026-04-19T10:00:00Z',
    ...overrides,
  } as unknown as BoardTask;
}

describe('runCli — list rendering', () => {
  it('groups cards by status and prints each', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'WIP', title: 'in-progress' }),
      mkTask({ index: 2, status: 'Next', title: 'queued' }),
      mkTask({ index: 3, status: 'Later', title: 'parked' }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'list'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/WIP \(1\)/);
    expect(joined).toMatch(/Next \(1\)/);
    expect(joined).toMatch(/Later \(1\)/);
    expect(joined).toMatch(/in-progress/);
  });

  it('--product chorus filters to only product:chorus tagged cards', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', title: 'chorus-card', domains: ['product:chorus'] }),
      mkTask({ index: 2, status: 'Next', title: 'gathering-card', domains: [] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', '--product', 'chorus', 'list'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/chorus-card/);
    expect(joined).not.toMatch(/gathering-card/);
  });

  it('--product gathering excludes product:chorus tagged cards', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', title: 'chorus-card', domains: ['product:chorus'] }),
      mkTask({ index: 2, status: 'Next', title: 'gathering-card', domains: [] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', '--product', 'gathering', 'list'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/gathering-card/);
    expect(joined).not.toMatch(/chorus-card/);
  });

  it('renders owner + priority + domain tags in bracket', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({
      index: 5, status: 'Next', title: 'tagged-card', owner: 'Silas', priority: 'P1',
      domains: ['domain:chorus', 'sequence:quality'],
    })];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'list'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Silas/);
    expect(joined).toMatch(/P1/);
    expect(joined).toMatch(/domain:chorus/);
  });
});

describe('runCli — filter', () => {
  it('filter with no args prints usage', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'Next' })];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/Usage:/);
    expect(joined).toMatch(/At least one filter required/);
  });

  it('filter --domain <d> returns matching cards', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', title: 'chorus-one', domains: ['domain:chorus'] }),
      mkTask({ index: 2, status: 'Next', title: 'photos-one', domains: ['domain:photos'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--domain', 'chorus'], factory(mock));
    } finally { cap.restore(); }
    const joined = cap.logs.join('\n');
    expect(joined).toMatch(/chorus-one/);
    expect(joined).not.toMatch(/photos-one/);
    expect(joined).toMatch(/1 cards/);
  });

  it('filter --owner <name> matches case-insensitively', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', owner: 'Kade', title: 'kade-card' }),
      mkTask({ index: 2, status: 'Next', owner: 'Wren', title: 'wren-card' }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--owner', 'KADE'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/kade-card/);
    expect(cap.logs.join('\n')).not.toMatch(/wren-card/);
  });

  it('filter --sequence <s> narrows by sequence tag', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Next', title: 'q-card', domains: ['sequence:quality'] }),
      mkTask({ index: 2, status: 'Next', title: 'o-card', domains: ['sequence:ops'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--sequence', 'quality'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/q-card/);
    expect(cap.logs.join('\n')).not.toMatch(/o-card/);
  });

  it('filter with no match prints "No cards match filters"', async () => {
    const mock = new MockClient();
    mock.tasks = [mkTask({ index: 1, status: 'Next', domains: ['domain:chorus'] })];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--domain', 'nonexistent'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/No cards match filters/);
  });

  it('filter --status narrows to one bucket', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'WIP', domains: ['domain:chorus'] }),
      mkTask({ index: 2, status: 'Next', domains: ['domain:chorus'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--domain', 'chorus', '--status', 'WIP'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/1 cards/);
  });

  it('filter excludes Done and Won\'t Do', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, status: 'Done', domains: ['domain:chorus'] }),
      mkTask({ index: 2, status: "Won't Do", domains: ['domain:chorus'] }),
      mkTask({ index: 3, status: 'Next', domains: ['domain:chorus'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--domain', 'chorus'], factory(mock));
    } finally { cap.restore(); }
    expect(cap.logs.join('\n')).toMatch(/1 cards/);
  });
});

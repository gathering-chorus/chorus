import { runCli } from '../src/cli';
import type { BoardClient } from '../src/client';
import type { BoardTask } from '../src/types';

class MockClient {
  boardName = 'gathering';
  tasks: BoardTask[] = [];

  async list() { return this.tasks; }
  async listGrouped(): Promise<Map<string, BoardTask[]>> {
    const m = new Map<string, BoardTask[]>();
    for (const t of this.tasks) {
      const s = (t as { status?: string }).status ?? 'Later';
      if (!m.has(s)) m.set(s, []);
      m.get(s)!.push(t);
    }
    return m;
  }
  async mine(role: string) { return this.tasks.filter((t) => (t.owner ?? '').toLowerCase() === role.toLowerCase()); }
  async listLabels() { return []; }
}

function factory(mock: MockClient) { return () => mock as unknown as BoardClient; }

function silence() {
  const origLog = console.log;
  const logs: string[] = [];
  console.log = (...a) => logs.push(a.join(' '));
  return { logs, restore: () => { console.log = origLog; } };
}

function mkTask(overrides: Partial<BoardTask>): BoardTask {
  return {
    index: 1, title: 'card', description: '', status: 'Next',
    owner: 'Kade', priority: 'P2', domains: [], apiId: 1000, done: false,
    created: '2026-04-22T10:00:00Z', updated: '2026-04-22T10:00:00Z',
    ...overrides,
  } as unknown as BoardTask;
}

describe('cards filter command', () => {
  test('filter by sequence returns only cards carrying that sequence', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, title: 'werk one', domains: ['sequence:werk', 'domain:chorus'] }),
      mkTask({ index: 2, title: 'strategy one', domains: ['sequence:strategy', 'domain:chorus'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--domain', 'chorus', '--sequence', 'werk'], factory(mock)).catch(() => {});
    } finally { cap.restore(); }
    const output = cap.logs.join('\n');
    expect(output).toContain('sequence:werk');
    expect(output).not.toContain('sequence:strategy');
  });

  test('filter by owner returns only cards owned by that role', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, title: 'wren card', owner: 'Wren' }),
      mkTask({ index: 2, title: 'kade card', owner: 'Kade' }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--owner', 'wren'], factory(mock)).catch(() => {});
    } finally { cap.restore(); }
    const output = cap.logs.join('\n');
    expect(output).toContain('Wren');
    expect(output).not.toContain('kade card');
  });

  test('filter by type returns only cards tagged with that type', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, title: 'fix one', domains: ['type:fix', 'domain:chorus'] }),
      mkTask({ index: 2, title: 'enhance one', domains: ['type:enhance', 'domain:chorus'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--domain', 'chorus', '--type', 'fix'], factory(mock)).catch(() => {});
    } finally { cap.restore(); }
    const output = cap.logs.join('\n');
    expect(output).toContain('type:fix');
    expect(output).not.toContain('enhance one');
  });

  test('filter with no flags prints Usage guidance', async () => {
    const mock = new MockClient();
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter'], factory(mock)).catch(() => {});
    } finally { cap.restore(); }
    const output = cap.logs.join('\n');
    expect(output).toContain('Usage');
  });

  test('filter excludes Done and skipped statuses by default', async () => {
    const mock = new MockClient();
    mock.tasks = [
      mkTask({ index: 1, title: 'active', status: 'Next', domains: ['sequence:werk'] }),
      mkTask({ index: 2, title: 'closed', status: 'Done', domains: ['sequence:werk'] }),
      mkTask({ index: 3, title: 'skipped', status: "Won't Do", domains: ['sequence:werk'] }),
    ];
    const cap = silence();
    try {
      await runCli(['node', 'cards', 'filter', '--sequence', 'werk'], factory(mock)).catch(() => {});
    } finally { cap.restore(); }
    const output = cap.logs.join('\n');
    expect(output).toContain('active');
    expect(output).not.toContain('closed');
    expect(output).not.toContain('skipped');
  });
});

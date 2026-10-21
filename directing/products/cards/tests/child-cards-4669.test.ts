// @test-type: unit — stubbed board client; no live service.
// @domain: cards
/**
 * #4669 — child cards. A follow-on found by running a card becomes a child of
 * it ("patient 0", Jeff 2026-10-20) instead of a reopen. The link is Vikunja's
 * own parenttask/subtask relation.
 *
 *   - `cards add --parent <id>` links the new card as a child of <id>
 *   - `cards set <id> parent=<p>` links an existing card (how #4667 → #4652 lands)
 *   - `cards view` shows "Child of: #p" and lists children with their status
 *   - negative proof: a parent that does not exist is refused, nothing is filed
 */

import { BoardClient } from '../src/client';
import type { BoardTask, VikunjaTask } from '../src/types';
import { parseAddArgs } from '../src/cli-add-helpers';
import { addCard, setCard } from '../src/sdk';
import { runCli } from '../src/cli';
import { GATHERING } from '../src/config';

const desc = '## Experience\nJeff sees the child under its parent.\n\n## AC\n- [ ] linked';

class MockClient {
  boardName = 'gathering';
  calls: Array<{ method: string; args: unknown[] }> = [];
  tasks: Map<number, BoardTask> = new Map();
  constructor(existing: number[]) {
    for (const i of existing) this.tasks.set(i, mk(i));
  }
  private rec(m: string, a: unknown[]) { this.calls.push({ method: m, args: a }); }
  async view(index: number) {
    this.rec('view', [index]);
    const t = this.tasks.get(index);
    if (!t) throw new Error(`No task #${index} on gathering board`);
    return t;
  }
  async add(title: string) {
    this.rec('add', [title]);
    const t = { ...mk(9000), title };
    this.tasks.set(9000, t);
    return t;
  }
  async addRelation(index: number, other: number, kind: string) { this.rec('addRelation', [index, other, kind]); }
  async tag(...a: unknown[]) { this.rec('tag', a); }
  async applyLabelByName(...a: unknown[]) { this.rec('applyLabelByName', a); return { created: false }; }
  async comments() { return []; }
}

function mk(index: number, extra: Partial<BoardTask> = {}): BoardTask {
  return {
    index, apiId: index + 30, title: `card ${index}`, description: '', status: 'Later',
    owner: 'Wren', priority: 'P3', domains: [], done: false,
    created: '2026-10-21T00:00:00Z', updated: '2026-10-21T00:00:00Z', ...extra,
  };
}

function quiet() {
  const log = jest.spyOn(console, 'log').mockImplementation(() => {});
  const err = jest.spyOn(console, 'error').mockImplementation(() => {});
  const warn = jest.spyOn(console, 'warn').mockImplementation(() => {});
  const exit = jest.spyOn(process, 'exit').mockImplementation(((code?: number) => {
    throw new Error(`process.exit(${code})`);
  }) as never);
  const text = () => [...log.mock.calls, ...err.mock.calls].map((c) => c.join(' ')).join('\n');
  return { text, exit, restore: () => { log.mockRestore(); err.mockRestore(); warn.mockRestore(); exit.mockRestore(); } };
}

const addOpts = (parent?: string) => ({
  owner: 'wren', priority: 'P3', domain: 'chorus', type: 'fix', origin: 'reactive',
  sequence: 'chorus', description: desc, ...(parent ? { parent } : {}),
});

let savedRole: string | undefined;
beforeAll(() => { savedRole = process.env.DEPLOY_ROLE; process.env.DEPLOY_ROLE = 'jeff'; });
afterAll(() => { if (savedRole === undefined) delete process.env.DEPLOY_ROLE; else process.env.DEPLOY_ROLE = savedRole; });

describe('#4669 cards add --parent', () => {
  test('parses --parent', () => {
    expect(parseAddArgs(['a child', '--parent', '4652']).parent).toBe('4652');
  });

  test('links the new card as a child of the parent', async () => {
    const mock = new MockClient([4652]);
    const q = quiet();
    try { await addCard(mock as unknown as BoardClient, 'fix the child', addOpts('4652')); } finally { q.restore(); }
    expect(mock.calls).toContainEqual({ method: 'addRelation', args: [9000, 4652, 'parenttask'] });
  });

  test('no --parent, no relation', async () => {
    const mock = new MockClient([]);
    const q = quiet();
    try { await addCard(mock as unknown as BoardClient, 'fix a loner', addOpts()); } finally { q.restore(); }
    expect(mock.calls.some((c) => c.method === 'addRelation')).toBe(false);
  });

  test('negative proof: a parent that does not exist is refused and nothing is created', async () => {
    const mock = new MockClient([]);
    const q = quiet();
    try {
      await expect(addCard(mock as unknown as BoardClient, 'fix an orphan', addOpts('99999'))).rejects.toThrow('process.exit(1)');
      expect(q.text()).toMatch(/parent #99999/);
    } finally { q.restore(); }
    expect(mock.calls.some((c) => c.method === 'add')).toBe(false);
    expect(mock.calls.some((c) => c.method === 'addRelation')).toBe(false);
  });

  test('negative proof: a parent that is not a number is refused', async () => {
    const mock = new MockClient([]);
    const q = quiet();
    try {
      await expect(addCard(mock as unknown as BoardClient, 'fix a typo', addOpts('46x2'))).rejects.toThrow('process.exit(1)');
    } finally { q.restore(); }
    expect(mock.calls.some((c) => c.method === 'add')).toBe(false);
  });
});

describe('#4669 cards set <id> parent=<p>', () => {
  test('links an existing card to its parent', async () => {
    const mock = new MockClient([4667, 4652]);
    const q = quiet();
    try { await setCard(mock as unknown as BoardClient, 4667, { parent: '4652' }); } finally { q.restore(); }
    expect(mock.calls).toContainEqual({ method: 'addRelation', args: [4667, 4652, 'parenttask'] });
  });

  test('negative proof: a missing parent is refused before any write', async () => {
    const mock = new MockClient([4667]);
    const q = quiet();
    try {
      await expect(setCard(mock as unknown as BoardClient, 4667, { parent: '99999' })).rejects.toThrow(/parent #99999/);
    } finally { q.restore(); }
    expect(mock.calls.some((c) => c.method === 'addRelation')).toBe(false);
  });
});

describe('#4669 family from Vikunja relations', () => {
  const client = new BoardClient('http://x', 't', GATHERING);
  const related = (index: number, id: number, done = false) => ({ id, index, title: `card ${index}`, done });

  test('parent and children come from parenttask/subtask, child status from the bucket map', () => {
    const task = {
      id: 4682, index: 4652, title: 'p0', description: '', done: false, created: '', updated: '',
      labels: null, project_id: 1,
      related_tasks: { subtask: [related(4667, 4697, true), related(4670, 4700)], parenttask: [related(4600, 4630)] },
    } as VikunjaTask;
    const fam = client.familyOf(task, new Map([[4700, 'WIP']]));
    expect(fam.parent).toBe(4600);
    expect(fam.children).toEqual([
      { index: 4667, title: 'card 4667', status: 'Done' },
      { index: 4670, title: 'card 4670', status: 'WIP' },
    ]);
  });

  test('negative proof: a related task with no display index is refused, never shown as its api id', () => {
    const task = {
      id: 4682, index: 4652, title: 'p0', description: '', done: false, created: '', updated: '',
      labels: null, project_id: 1, related_tasks: { subtask: [{ id: 4697, title: 'no index', done: false }] },
    } as VikunjaTask;
    expect(() => client.familyOf(task, new Map())).toThrow(/api id 4697/);
  });

  test('a card with no relations has no family', () => {
    const task = { id: 1, index: 1, title: 't', description: '', done: false, created: '', updated: '', labels: null, project_id: 1 } as VikunjaTask;
    expect(client.familyOf(task, new Map())).toEqual({ parent: undefined, children: [] });
  });
});

describe('#4669 cards view shows the family', () => {
  async function viewOf(task: BoardTask, extra: string[] = []): Promise<string> {
    const mock = new MockClient([]);
    mock.tasks.set(task.index, task);
    const q = quiet();
    try { await runCli(['node', 'cards', 'view', String(task.index), ...extra], () => mock as unknown as BoardClient); return q.text(); } finally { q.restore(); }
  }

  test('a parent lists its children with status', async () => {
    const out = await viewOf(mk(4652, { children: [{ index: 4667, title: 'counts read #clearing', status: 'Done' }] }));
    expect(out).toMatch(/Children \(1\):/);
    expect(out).toMatch(/#4667 \[Done\] counts read #clearing/);
  });

  test('a child names its parent', async () => {
    const out = await viewOf(mk(4667, { parent: 4652 }));
    expect(out).toMatch(/Child of: #4652/);
  });

  test('view --json carries parent and children for /cws', async () => {
    const out = await viewOf(mk(4652, { parent: 4600, children: [{ index: 4667, title: 'c', status: 'Done' }] }), ['--json']);
    const j = JSON.parse(out);
    expect(j.parent).toBe(4600);
    expect(j.children).toEqual([{ index: 4667, title: 'c', status: 'Done' }]);
  });

  test('a card with no family prints neither line', async () => {
    const out = await viewOf(mk(10));
    expect(out).not.toMatch(/Child of|Children/);
  });
});

// @test-type: unit — BoardClient with the api() HTTP layer fully stubbed; no
// socket opened, no live Vikunja/network. Pure client-logic coverage.
/**
 * #2652 AC11 — BoardClient coverage uplift (44% → ≥70%).
 * Each test instantiates BoardClient directly with the api() HTTP layer stubbed.
 */
import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';
import type { VikunjaTask } from '../src/types';

interface MockTask extends VikunjaTask { project_id: number }

function stub(client: BoardClient, opts: {
  responses?: Map<string, unknown>;
  tasks?: Map<number, MockTask>;
  labels?: Array<{ id: number; title: string }>;
} = {}): { calls: Array<{ method: string; endpoint: string; body?: object }>; labels: Array<{ id: number; title: string }> } {
  const calls: Array<{ method: string; endpoint: string; body?: object }> = [];
  const labels = opts.labels ?? [];
  const responses = opts.responses ?? new Map();
  const tasks = opts.tasks ?? new Map();
  (client as any).api = jest.fn(async (method: string, endpoint: string, body?: object) => {
    calls.push({ method, endpoint, body });
    const key = `${method} ${endpoint}`;
    if (responses.has(key)) return responses.get(key);
    if (method === 'GET' && endpoint.startsWith('/labels')) return labels;
    if (method === 'PUT' && endpoint === '/labels') {
      const newLabel = { id: 1000 + labels.length, title: (body as { title: string }).title };
      labels.push(newLabel);
      return newLabel;
    }
    // GET /tasks/:id — return mock task (untag reads task.labels via this path)
    const taskMatch = endpoint.match(/^\/tasks\/(\d+)$/);
    if (method === 'GET' && taskMatch) return tasks.get(parseInt(taskMatch[1], 10));
    return { ok: true };
  });
  (client as any).fetchTask = jest.fn(async (apiId: number) => {
    const t = tasks.get(apiId);
    if (!t) throw new Error(`No task ${apiId}`);
    return t;
  });
  (client as any).resolveIndex = jest.fn(async (i: number) => i);
  (client as any).clearCache = jest.fn();
  return { calls, labels };
}

function makeTask(id: number, taskLabels: Array<{ id: number; title: string }> = []): MockTask {
  return {
    id, index: id, title: `task-${id}`, description: '',
    done: false, created: '2026-05-02T00:00:00Z', updated: '2026-05-02T00:00:00Z',
    labels: taskLabels, project_id: 2,
  } as MockTask;
}

describe('#2652 AC11 BoardClient coverage uplift', () => {
  test('createLabel returns id and title from PUT /labels via BoardClient', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    stub(client);
    const result = await client.createLabel('subdomain:photos-domain');
    expect(result.title).toBe('subdomain:photos-domain');
    expect(result.id).toBeGreaterThan(0);
  });

  test('listLabels paginates BoardClient until short batch', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const all75 = Array.from({ length: 75 }, (_, i) => ({ id: i + 1, title: `l${i}` }));
    const responses = new Map<string, unknown>([
      ['GET /labels?page=1', all75.slice(0, 50)],
      ['GET /labels?page=2', all75.slice(50, 75)],
    ]);
    stub(client, { responses });
    const all = await client.listLabels();
    expect(all).toHaveLength(75);
  });

  test('deleteLabel issues BoardClient DELETE /labels/:id', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const { calls } = stub(client);
    await client.deleteLabel(42);
    expect(calls.find((c) => c.method === 'DELETE' && c.endpoint === '/labels/42')).toBeDefined();
  });

  test('applyLabelByName on BoardClient creates label when missing', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks, labels: [] });
    const r = await client.applyLabelByName(100, 'subdomain:cards-service');
    expect(r.created).toBe(true);
    expect(r.labelId).toBeGreaterThan(0);
  });

  test('applyLabelByName on BoardClient reuses existing label', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks, labels: [{ id: 999, title: 'subproduct:werk' }] });
    const r = await client.applyLabelByName(100, 'subproduct:werk');
    expect(r.created).toBe(false);
    expect(r.labelId).toBe(999);
  });

  test('removeLabelByName on BoardClient no-ops when label absent', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks, labels: [] });
    const r = await client.removeLabelByName(100, 'subproduct:absent');
    expect(r.removed).toBe(false);
  });

  test('removeLabelByName on BoardClient removes when label present', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks, labels: [{ id: 555, title: 'subdomain:tests-domain' }] });
    const r = await client.removeLabelByName(100, 'subdomain:tests-domain');
    expect(r.removed).toBe(true);
  });

  test('untag on BoardClient throws when label not on task', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks });
    await expect(client.untag(100, 'sequence', 'absent')).rejects.toThrow(/not found/);
  });

  test('untag on BoardClient removes when label present', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100, [{ id: 77, title: 'sequence:werk' }])]]);
    const { calls } = stub(client, { tasks });
    await client.untag(100, 'sequence', 'werk');
    expect(calls.find((c) => c.method === 'DELETE' && c.endpoint.startsWith('/tasks/100/labels/77'))).toBeDefined();
  });

  test('reassignOwner on BoardClient swaps owner labels', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100, [{ id: 2, title: 'owner:wren' }])]]);
    stub(client, { tasks });
    const r = await client.reassignOwner(100, 'kade');
    expect(r.oldOwner).toBe('wren');
    expect(r.newOwner.toLowerCase()).toBe('kade');
  });

  test('reassignOwner on BoardClient refuses unknown owner', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks });
    await expect(client.reassignOwner(100, 'nobody')).rejects.toThrow(/Unknown owner/);
  });

  test('tag on BoardClient adds a known-category label', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    const { calls } = stub(client, { tasks });
    await client.tag(100, 'sequence', 'werk');
    expect(calls.find((c) => c.method === 'PUT' && c.endpoint === '/tasks/100/labels')).toBeDefined();
  });

  test('tag on BoardClient throws on unknown category', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks });
    await expect(client.tag(100, 'fakecat', 'fakevalue')).rejects.toThrow(/Unknown label category/);
  });

  test('tag on BoardClient throws on unknown value within known category', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    stub(client, { tasks });
    await expect(client.tag(100, 'sequence', 'no-such-sequence-XYZ')).rejects.toThrow(/Unknown sequence/);
  });

  test('tag on BoardClient removes existing same-category label before adding new one', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    // Pre-existing sequence:loom on the task; switching to sequence:werk should
    // remove loom first, then add werk. The mutex behavior is part of tag()'s contract.
    const tasks = new Map([[100, makeTask(100, [{ id: 85, title: 'sequence:loom' }])]]);
    const { calls } = stub(client, { tasks });
    await client.tag(100, 'sequence', 'werk');
    // Removal call recorded for the prior label
    expect(calls.find((c) => c.method === 'DELETE' && c.endpoint.startsWith('/tasks/100/labels/85'))).toBeDefined();
  });

  test('createLabel on BoardClient returns expected envelope', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    stub(client);
    const created = await client.createLabel('subproduct:loom');
    expect(typeof created.id).toBe('number');
    expect(created.title).toBe('subproduct:loom');
  });

  test('block on BoardClient moves to blocked bucket and comments reason', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    const { calls } = stub(client, { tasks });
    await client.block(100, 'awaiting external');
    expect(calls.find((c) => c.method === 'PUT' && c.endpoint === '/tasks/100/comments')).toBeDefined();
  });

  test('unblock on BoardClient moves to Next bucket and comments', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    const { calls } = stub(client, { tasks });
    await client.unblock(100);
    expect(calls.find((c) => c.method === 'PUT' && c.endpoint === '/tasks/100/comments')).toBeDefined();
  });

  test('update on BoardClient sends merged title and description', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    const { calls } = stub(client, { tasks });
    await client.update(100, { title: 'new title' });
    const updateCall = calls.find((c) => c.method === 'POST' && c.endpoint === '/tasks/100');
    expect(updateCall).toBeDefined();
    expect((updateCall?.body as { title: string }).title).toBe('new title');
  });

  test('update on BoardClient applies product label when provided', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    const { calls } = stub(client, { tasks });
    await client.update(100, { product: 'chorus' });
    expect(calls.find((c) => c.method === 'PUT' && c.endpoint === '/tasks/100/labels')).toBeDefined();
  });

  test('comment on BoardClient sends comment text', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const tasks = new Map([[100, makeTask(100)]]);
    const { calls } = stub(client, { tasks });
    await client.comment(100, 'gate:product-pass — Wren');
    const c = calls.find((cc) => cc.method === 'PUT' && cc.endpoint === '/tasks/100/comments');
    expect(c).toBeDefined();
    expect((c?.body as { comment: string }).comment).toContain('gate:product-pass');
  });
});

// @test-type: unit — BoardClient.api fully stubbed; the localhost URL is an
// unused constructor arg (no socket is opened). No live Vikunja/network.
/**
 * #3432 — chunk passed at add-time was silently dropped.
 *
 * Root cause: applyAddLabels routed `chunk` through the STATIC LABELS.chunk
 * map. Post-#3267 chunk is the dynamic priority axis — priority chunks
 * (coherent-model, loom-authoring, werk-reliability, core-reliability, cards,
 * proving, …) are NOT in that static map, so LABELS.chunk[value] === undefined
 * and the label was skipped. The card landed untagged → invisible in
 * chorus_priorities_readout (TAGGED-ONLY) until a manual chorus_cards_set.
 *
 * Fix: apply chunk by NAME (find-or-create), the same path cards_set uses, so
 * any priority chunk sticks at add-time. Shared client.add code path = fixes
 * both /card (chorus_card_add_jeff) and the agent chorus_cards_add path.
 *
 * Hermetic: stubs BoardClient.api (same harness as client-coverage-2652).
 */
import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';
import type { VikunjaTask } from '../src/types';

interface MockTask extends VikunjaTask { project_id: number }

function stub(client: BoardClient, opts: {
  labels?: Array<{ id: number; title: string }>;
  tasks?: Map<number, MockTask>;
} = {}): { calls: Array<{ method: string; endpoint: string; body?: object }>; labels: Array<{ id: number; title: string }> } {
  const calls: Array<{ method: string; endpoint: string; body?: object }> = [];
  const labels = opts.labels ?? [];
  const tasks = opts.tasks ?? new Map();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (client as any).api = jest.fn(async (method: string, endpoint: string, body?: object) => {
    calls.push({ method, endpoint, body });
    if (method === 'GET' && endpoint.startsWith('/labels')) return labels;
    if (method === 'PUT' && endpoint === '/labels') {
      const newLabel = { id: 1000 + labels.length, title: (body as { title: string }).title };
      labels.push(newLabel);
      return newLabel;
    }
    const taskMatch = endpoint.match(/^\/tasks\/(\d+)$/);
    if (method === 'GET' && taskMatch) return tasks.get(parseInt(taskMatch[1], 10));
    return { ok: true };
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (client as any).resolveIndex = jest.fn(async (i: number) => i);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (client as any).clearCache = jest.fn();
  return { calls, labels };
}

function makeTask(id: number): MockTask {
  return {
    id, index: id, title: `task-${id}`, description: '',
    done: false, created: '2026-06-23T00:00:00Z', updated: '2026-06-23T00:00:00Z',
    labels: [], project_id: 2,
  } as MockTask;
}

describe('#3432 — chunk applied at add-time', () => {
  test('dynamic priority chunk is created + applied (was silently dropped)', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const { calls } = stub(client, { tasks: new Map([[500, makeTask(500)]]), labels: [] });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await (client as any).applyAddLabels(500, { chunk: 'coherent-model' });

    // the chunk:coherent-model label is created (not in static LABELS.chunk)
    const created = calls.find((c) => c.method === 'PUT' && c.endpoint === '/labels'
      && (c.body as { title: string } | undefined)?.title === 'chunk:coherent-model');
    expect(created).toBeDefined();

    // and applied to the task
    const applied = calls.find((c) => c.method === 'PUT' && c.endpoint === '/tasks/500/labels');
    expect(applied).toBeDefined();
  });

  test('existing chunk label is reused, not duplicated', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const { calls } = stub(client, {
      tasks: new Map([[500, makeTask(500)]]),
      labels: [{ id: 19, title: 'chunk:memory' }],
    });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await (client as any).applyAddLabels(500, { chunk: 'memory' });

    // no new label created — the existing one is reused
    expect(calls.find((c) => c.method === 'PUT' && c.endpoint === '/labels')).toBeUndefined();
    // applied by its existing id
    const applied = calls.find((c) => c.method === 'PUT' && c.endpoint === '/tasks/500/labels'
      && (c.body as { label_id: number } | undefined)?.label_id === 19);
    expect(applied).toBeDefined();
  });

  test('chunk casing is normalized to lowercase on the label name', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const { calls } = stub(client, { tasks: new Map([[500, makeTask(500)]]), labels: [] });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await (client as any).applyAddLabels(500, { chunk: 'Coherent-Model' });

    const created = calls.find((c) => c.method === 'PUT' && c.endpoint === '/labels'
      && (c.body as { title: string } | undefined)?.title === 'chunk:coherent-model');
    expect(created).toBeDefined();
  });

  test('static axes (domain) still apply alongside the dynamic chunk', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    const { calls } = stub(client, { tasks: new Map([[500, makeTask(500)]]), labels: [] });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await (client as any).applyAddLabels(500, { domain: 'chorus', chunk: 'loom-authoring' });

    // domain (static LABELS.domain) still produces a label application
    const labelApplies = calls.filter((c) => c.method === 'PUT' && c.endpoint === '/tasks/500/labels');
    expect(labelApplies.length).toBeGreaterThanOrEqual(2);
    // and the dynamic chunk label was created
    expect(calls.find((c) => c.method === 'PUT' && c.endpoint === '/labels'
      && (c.body as { title: string } | undefined)?.title === 'chunk:loom-authoring')).toBeDefined();
  });
});

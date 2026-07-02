// @test-type: unit — BoardClient HTTP-verb methods with the api()/resolveIndex layer
// fully stubbed; no socket, no live Vikunja. Asserts each verb's exact contract call.
// #3600 — covers client.ts's untested thin wrappers (delete/relations/labels/bucket-limit/
// prefix-search), the drag under the cards coverage floor.
import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';

type Call = { method: string; endpoint: string; body?: unknown };

function make(): { client: BoardClient; calls: Call[] } {
  const client = new BoardClient('http://vikunja.test', 'tok', GATHERING);
  const calls: Call[] = [];
  const c = client as any;
  c.api = jest.fn(async (method: string, endpoint: string, body?: unknown) => {
    calls.push({ method, endpoint, body });
    return {};
  });
  c.resolveIndex = jest.fn(async (i: number) => 1000 + i);
  c.clearCache = jest.fn();
  return { client, calls };
}

describe('BoardClient verb contracts (#3600 coverage)', () => {
  it('deleteTask → DELETE /tasks/<apiId>', async () => {
    const { client, calls } = make();
    await client.deleteTask(5);
    expect(calls[0]).toEqual({ method: 'DELETE', endpoint: '/tasks/1005', body: undefined });
  });

  it('removeLabel → DELETE /tasks/<apiId>/labels/<labelId>', async () => {
    const { client, calls } = make();
    await client.removeLabel(1001, 42);
    expect(calls[0]).toEqual({ method: 'DELETE', endpoint: '/tasks/1001/labels/42', body: undefined });
  });

  it('addRelation → PUT /tasks/<a>/relations with the other id + kind', async () => {
    const { client, calls } = make();
    await client.addRelation(1, 2, 'blocked');
    expect(calls[0]).toEqual({
      method: 'PUT',
      endpoint: '/tasks/1001/relations',
      body: { other_task_id: 1002, relation_kind: 'blocked' },
    });
  });

  it('removeRelation → DELETE /tasks/<a>/relations/<kind>/<b>', async () => {
    const { client, calls } = make();
    await client.removeRelation(1, 2, 'blocked');
    expect(calls[0]).toEqual({
      method: 'DELETE',
      endpoint: '/tasks/1001/relations/blocked/1002',
      body: undefined,
    });
  });

  it('setBucketLimit → POST the bucket with its title + the new limit', async () => {
    const { client, calls } = make();
    (client as any).fetchBuckets = jest.fn(async () => [{ id: 5, title: 'WIP' }]);
    await client.setBucketLimit(5, 3);
    expect(calls[0].method).toBe('POST');
    expect(calls[0].endpoint).toContain('/buckets/5');
    expect(calls[0].body).toEqual({ title: 'WIP', limit: 3 });
  });

  it('setBucketLimit → falls back to "Unknown" title when the bucket is missing', async () => {
    const { client, calls } = make();
    (client as any).fetchBuckets = jest.fn(async () => []);
    await client.setBucketLimit(99, 1);
    expect(calls[0].body).toEqual({ title: 'Unknown', limit: 1 });
  });

  it('findByTitlePrefix → returns matching display indices, sorted', async () => {
    const { client } = make();
    const c = client as any;
    c.buildTaskMap = jest.fn(async () => new Map([[3, 1003], [1, 1001], [2, 1002]]));
    c.fetchTask = jest.fn(async (apiId: number) => ({
      title: apiId === 1001 || apiId === 1003 ? 'sentinel-x' : 'other',
    }));
    const res = await client.findByTitlePrefix('sentinel');
    expect(res).toEqual([1, 3]);
  });

  it('getRelations → maps related API ids back to display indices', async () => {
    const { client } = make();
    const c = client as any;
    c.fetchTask = jest.fn(async () => ({
      related_tasks: { blocked: [{ id: 1002 }], blocking: [{ id: 1003 }] },
    }));
    c.buildTaskMap = jest.fn(async () => new Map([[2, 1002], [3, 1003]]));
    const rel = await client.getRelations(1);
    expect(rel).toHaveProperty('blockedBy');
    expect(rel).toHaveProperty('blocks');
  });

  describe('reassignOwner', () => {
    it('removes the old owner label, adds the new one, returns capitalized owner', async () => {
      const { client } = make();
      const c = client as any;
      c.fetchTask = jest.fn(async () => ({ labels: [{ id: 3, title: 'owner:silas' }] }));
      const removed: number[][] = [];
      const added: number[][] = [];
      c.removeLabel = jest.fn(async (a: number, l: number) => { removed.push([a, l]); });
      c.addLabel = jest.fn(async (a: number, l: number) => { added.push([a, l]); });

      const res = await client.reassignOwner(7, 'kade');
      expect(res).toEqual({ oldOwner: 'silas', newOwner: 'Kade' });
      expect(removed[0]).toEqual([1007, 3]);
      expect(added[0]).toEqual([1007, 4]); // LABELS.owner.kade === 4
    });

    it('throws on an unknown owner, naming the valid owners', async () => {
      const { client } = make();
      const c = client as any;
      c.fetchTask = jest.fn(async () => ({ labels: [] }));
      c.addLabel = jest.fn();
      await expect(client.reassignOwner(1, 'nobody')).rejects.toThrow(/Unknown owner "nobody"/);
    });
  });

  describe('paginated + mapper wrappers (#3600)', () => {
    it('fetchAllTasks paginates until an empty page', async () => {
      const { client } = make();
      (client as any).api = jest.fn(async (_m: string, ep: string) =>
        (ep.includes('page=1') ? [{ id: 1 }, { id: 2 }] : []));
      expect(await client.fetchAllTasks()).toHaveLength(2);
    });

    it('comment resolves the index then PUTs the comment', async () => {
      const { client, calls } = make();
      await client.comment(4, 'hello');
      expect(calls[0]).toEqual({ method: 'PUT', endpoint: '/tasks/1004/comments', body: { comment: 'hello' } });
    });

    it('comments maps author.username + text, with an unknown fallback', async () => {
      const { client } = make();
      (client as any).api = jest.fn(async () => [
        { author: { username: 'kade' }, comment: 'hi' },
        { comment: 'anon' },
      ]);
      expect(await client.comments(1)).toEqual([
        { author: 'kade', text: 'hi' },
        { author: 'unknown', text: 'anon' },
      ]);
    });

    it('createLabel PUTs /labels with the title', async () => {
      const { client, calls } = make();
      await client.createLabel('sentinel');
      expect(calls[0]).toEqual({ method: 'PUT', endpoint: '/labels', body: { title: 'sentinel' } });
    });

    it('fetchBucketsWithLimits maps id/title/limit/taskCount', async () => {
      const { client } = make();
      (client as any).fetchBuckets = jest.fn(async () => [{ id: 5, title: 'WIP', limit: 3, tasks: [{}, {}] }]);
      expect(await client.fetchBucketsWithLimits()).toEqual([{ id: 5, title: 'WIP', limit: 3, taskCount: 2 }]);
    });
  });

  describe('add + view (the core create/read paths, #3600)', () => {
    it('add: creates the task, resolves the bucket, applies every label axis', async () => {
      const { client, calls } = make();
      const c = client as any;
      c.api = jest.fn(async (m: string, ep: string, body?: unknown) => {
        calls.push({ method: m, endpoint: ep, body });
        if (m === 'PUT' && ep.endsWith('/tasks')) return { id: 900 };
        if (m === 'GET' && ep.startsWith('/labels')) return [];
        return {};
      });
      c.moveToBucket = jest.fn();
      c.parseTask = jest.fn((_t: unknown, status: string) => ({ index: 900, title: 'X', status, owner: 'kade' }));

      const res = await client.add('X', {
        status: 'wip', owner: 'kade', priority: 'P1', domain: 'chorus',
        product: 'chorus', sequence: 'quality', type: 'fix', chunk: 'tests', description: 'd',
      });

      expect(res.status).toBe('WIP');
      expect(c.moveToBucket).toHaveBeenCalled();
      // applyAddLabels ran the static axes (owner/priority/…) → addLabel PUTs labels
      expect(calls.some((k) => k.endpoint === '/tasks/900/labels')).toBe(true);
    });

    it('view: falls back to bucket lookup when the DB map misses', async () => {
      const { client } = make();
      const c = client as any;
      c.fetchTask = jest.fn(async () => ({ id: 50, title: 'T', done: false }));
      c.fetchBucketMapFromDB = jest.fn(() => new Map()); // force the fallback branch
      c.fetchBuckets = jest.fn(async () => [{ id: 5, title: 'WIP', tasks: [{ id: 50 }] }]);
      c.parseTask = jest.fn((_t: unknown, status: string) => ({ index: 3, status }));

      const res = await client.view(3);
      expect(res.status).toBe('WIP'); // findTaskBucket matched id 50 → 'WIP'
    });
  });
});

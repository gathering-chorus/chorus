import { BoardClient } from '../src/client';
import { GATHERING, SELF } from '../src/config';
import { VikunjaBucket } from '../src/types';

// Mock HTTP to avoid real API calls in unit tests
const mockBuckets: VikunjaBucket[] = [
  {
    id: 5, title: 'Now', limit: 3, tasks: [
      {
        id: 101, index: 1, title: 'Build feature', description: 'Build it',
        done: false, created: '2026-02-20T10:00:00Z', updated: '2026-02-20T12:00:00Z',
        labels: [{ id: 2, title: 'owner:wren' }, { id: 5, title: 'P1' }],
        project_id: 2,
      },
      {
        id: 104, index: 4, title: 'Fix bug', description: 'Fix it',
        done: false, created: '2026-02-20T11:00:00Z', updated: '2026-02-20T13:00:00Z',
        labels: [{ id: 4, title: 'owner:kade' }, { id: 6, title: 'P2' }],
        project_id: 2,
      },
    ],
  },
  {
    id: 7, title: 'Next', limit: 0, tasks: [
      {
        id: 102, index: 2, title: 'Review docs', description: '',
        done: false, created: '2026-02-20T09:00:00Z', updated: '2026-02-20T09:00:00Z',
        labels: [{ id: 3, title: 'owner:silas' }, { id: 6, title: 'P2' }, { id: 12, title: 'gathering' }],
        project_id: 2,
      },
    ],
  },
  {
    id: 6, title: 'Done', limit: 0, tasks: [
      {
        id: 103, index: 3, title: 'Ship v1', description: 'Shipped',
        done: true, created: '2026-02-19T10:00:00Z', updated: '2026-02-20T08:00:00Z',
        labels: [{ id: 4, title: 'owner:kade' }, { id: 5, title: 'P1' }],
        project_id: 2,
      },
    ],
  },
  { id: 4, title: 'Later', limit: 0, tasks: null },
  { id: 8, title: 'Blocked', limit: 0, tasks: [] },
];

// Create a client and override fetchBuckets to use mock data
function createMockClient(): BoardClient {
  const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
  // Override the private fetchBuckets method
  (client as any).api = jest.fn().mockImplementation((method: string, endpoint: string) => {
    if (endpoint.includes('/views/') && endpoint.includes('/tasks')) {
      return Promise.resolve(mockBuckets);
    }
    if (endpoint.match(/^\/tasks\/\d+$/) && method === 'GET') {
      const id = parseInt(endpoint.split('/').pop()!);
      for (const b of mockBuckets) {
        for (const t of b.tasks || []) {
          if (t.id === id) return Promise.resolve(t);
        }
      }
    }
    return Promise.resolve({});
  });
  return client;
}

describe('BoardClient.list', () => {
  test('returns all tasks across buckets', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    expect(tasks).toHaveLength(4);
  });

  test('parses owner from labels', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    const wrenTask = tasks.find(t => t.index === 1);
    expect(wrenTask?.owner).toBe('Wren');
  });

  test('parses priority from labels', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    const t = tasks.find(t => t.index === 1);
    expect(t?.priority).toBe('P1');
  });

  test('parses domain labels', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    const t = tasks.find(t => t.index === 2);
    expect(t?.domains).toContain('gathering');
  });

  test('sets status from bucket name', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    expect(tasks.find(t => t.index === 1)?.status).toBe('Now');
    expect(tasks.find(t => t.index === 2)?.status).toBe('Next');
    expect(tasks.find(t => t.index === 3)?.status).toBe('Done');
  });

  test('handles null tasks array in bucket', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    // 'Later' bucket has null tasks — should not crash, just skip
    expect(tasks.every(t => t.status !== 'Later')).toBe(true);
  });

  test('handles empty tasks array in bucket', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    // 'Blocked' bucket has [] — should not crash
    expect(tasks.every(t => t.status !== 'Blocked')).toBe(true);
  });
});

describe('BoardClient.listGrouped', () => {
  test('groups tasks by status', async () => {
    const client = createMockClient();
    const grouped = await client.listGrouped();
    expect(grouped.get('Now')).toHaveLength(2);
    expect(grouped.get('Next')).toHaveLength(1);
    expect(grouped.get('Done')).toHaveLength(1);
    expect(grouped.has('Later')).toBe(false); // null tasks
    expect(grouped.has('Blocked')).toBe(false); // empty tasks
  });
});

describe('BoardClient.mine', () => {
  test('filters by role', async () => {
    const client = createMockClient();
    const wrenTasks = await client.mine('wren');
    expect(wrenTasks).toHaveLength(1);
    expect(wrenTasks[0].title).toBe('Build feature');
  });

  test('returns empty for role with no tasks', async () => {
    const client = createMockClient();
    const jeffTasks = await client.mine('jeff');
    expect(jeffTasks).toHaveLength(0);
  });

  test('is case-insensitive', async () => {
    const client = createMockClient();
    const tasks = await client.mine('Wren');
    expect(tasks).toHaveLength(1);
  });
});

describe('BoardClient.resolveIndex', () => {
  test('maps display index to API ID', async () => {
    const client = createMockClient();
    const apiId = await client.resolveIndex(1);
    expect(apiId).toBe(101);
  });

  test('throws for unknown index', async () => {
    const client = createMockClient();
    await expect(client.resolveIndex(999)).rejects.toThrow('No task #999');
  });
});

describe('BoardClient.view', () => {
  test('returns parsed task by index', async () => {
    const client = createMockClient();
    const task = await client.view(1);
    expect(task.title).toBe('Build feature');
    expect(task.status).toBe('Now');
    expect(task.owner).toBe('Wren');
    expect(task.priority).toBe('P1');
  });
});

describe('BoardClient.snapshot', () => {
  test('captures board name and timestamp', async () => {
    const client = createMockClient();
    const snap = await client.snapshot();
    expect(snap.board).toBe('gathering');
    expect(snap.timestamp).toBeDefined();
    expect(new Date(snap.timestamp).getTime()).not.toBeNaN();
  });

  test('includes all tasks', async () => {
    const client = createMockClient();
    const snap = await client.snapshot();
    expect(snap.tasks).toHaveLength(4);
  });
});

describe('BoardClient cache', () => {
  test('clearCache forces re-fetch on next resolveIndex', async () => {
    const client = createMockClient();
    const api = (client as any).api as jest.Mock;

    await client.resolveIndex(1);
    const callCount = api.mock.calls.length;

    // Same call should use cache
    await client.resolveIndex(2);
    expect(api.mock.calls.length).toBe(callCount);

    // After clear, should re-fetch
    client.clearCache();
    await client.resolveIndex(1);
    expect(api.mock.calls.length).toBeGreaterThan(callCount);
  });
});

describe('BoardClient.now', () => {
  test('returns Now tasks for a specific role', async () => {
    const client = createMockClient();
    const tasks = await client.now('wren');
    expect(tasks).toHaveLength(1);
    expect(tasks[0].title).toBe('Build feature');
    expect(tasks[0].status).toBe('Now');
  });

  test('returns empty for role with nothing in Now', async () => {
    const client = createMockClient();
    const tasks = await client.now('silas');
    expect(tasks).toHaveLength(0);
  });

  test('is case-insensitive', async () => {
    const client = createMockClient();
    const tasks = await client.now('Kade');
    expect(tasks).toHaveLength(1);
    expect(tasks[0].title).toBe('Fix bug');
  });

  test('does not include non-Now tasks', async () => {
    const client = createMockClient();
    // Kade has a Done task (#3) and a Now task (#4)
    const tasks = await client.now('kade');
    expect(tasks).toHaveLength(1);
    expect(tasks[0].status).toBe('Now');
  });
});

describe('BoardClient.fetchBucketsWithLimits', () => {
  test('returns bucket metadata with limits', async () => {
    const client = createMockClient();
    const buckets = await client.fetchBucketsWithLimits();
    expect(buckets).toHaveLength(5);

    const nowBucket = buckets.find(b => b.title === 'Now');
    expect(nowBucket).toBeDefined();
    expect(nowBucket!.limit).toBe(3);
    expect(nowBucket!.taskCount).toBe(2);
  });

  test('returns 0 for unlimited buckets', async () => {
    const client = createMockClient();
    const buckets = await client.fetchBucketsWithLimits();
    const nextBucket = buckets.find(b => b.title === 'Next');
    expect(nextBucket!.limit).toBe(0);
  });

  test('handles null tasks array as 0 count', async () => {
    const client = createMockClient();
    const buckets = await client.fetchBucketsWithLimits();
    const laterBucket = buckets.find(b => b.title === 'Later');
    expect(laterBucket!.taskCount).toBe(0);
  });
});

describe('BoardClient.moveToBucket error handling', () => {
  test('translates 412 bucket limit error to friendly message', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    (client as any).api = jest.fn().mockImplementation((method: string, endpoint: string) => {
      if (endpoint.includes('/views/') && endpoint.includes('/buckets/') && endpoint.includes('/tasks')) {
        return Promise.reject(new Error('API POST /endpoint: 412 {"message":"bucket limit reached"}'));
      }
      if (endpoint.includes('/views/') && endpoint.includes('/tasks') && !endpoint.includes('/buckets/')) {
        return Promise.resolve(mockBuckets);
      }
      return Promise.resolve({});
    });

    await expect((client as any).moveToBucket(101, 5)).rejects.toThrow('Now column is full');
    await expect((client as any).moveToBucket(101, 5)).rejects.toThrow('Move something to Done first');
  });

  test('passes through non-412 errors unchanged', async () => {
    const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
    (client as any).api = jest.fn().mockImplementation(() => {
      return Promise.reject(new Error('API POST /endpoint: 500 internal error'));
    });

    await expect((client as any).moveToBucket(101, 5)).rejects.toThrow('500');
  });
});

describe('BoardClient constructor', () => {
  test('strips trailing slash from URL', () => {
    const client = new BoardClient('http://localhost:3456/', 'token', GATHERING);
    expect(client.boardName).toBe('gathering');
  });

  test('exposes board config', () => {
    const client = new BoardClient('http://localhost:3456', 'token', SELF);
    expect(client.config).toBe(SELF);
    expect(client.boardName).toBe('self');
  });
});

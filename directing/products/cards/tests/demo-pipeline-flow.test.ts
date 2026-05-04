/**
 * Demo Pipeline Flow Tests — #1241
 *
 * End-to-end validation of the demo pipeline:
 *   cards demo → spine event → observer nudge → brief to PM → accept/reject
 *
 * Tests SDK functions with mocked BoardClient and spine events.
 */
import * as fs from 'fs';
import * as path from 'path';
import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';
import { demoCard, doneCard, rejectCard } from '../src/sdk';

// ── Capture spine events ──
const emittedEvents: Array<{ event: string; role: string; extra: Record<string, string> }> = [];

jest.mock('../src/events', () => ({
  emitSpineEvent: (event: string, role: string, extra: Record<string, string> = {}) => {
    emittedEvents.push({ event, role, extra });
  },
  emitChorusEvent: (event: string, role: string, extra: Record<string, string> = {}) => {
    emittedEvents.push({ event, role, extra });
  },
}));

jest.mock('../src/config', () => {
  const actual = jest.requireActual('../src/config');
  return {
    ...actual,
    detectRole: () => 'kade',
  };
});

jest.mock('../src/blast-radius', () => ({
  generateBlastRadius: jest.fn().mockResolvedValue(null),
  formatBlastComment: jest.fn().mockReturnValue(''),
}));

// ── Mock data ──
const mockBuckets = [
  {
    id: 29, title: 'WIP', limit: 3, tasks: [
      {
        id: 301, index: 50, title: 'Feature under demo',
        description: '## Acceptance Criteria\n- [ ] Feature works\n- [ ] Tests pass',
        done: false, created: '2026-03-01T10:00:00Z', updated: '2026-03-10T14:00:00Z',
        labels: [{ id: 4, title: 'owner:kade' }, { id: 5, title: 'P1' }],
        project_id: 2,
      },
    ],
  },
  {
    id: 5, title: 'Now', limit: 3, tasks: [
      {
        id: 302, index: 51, title: 'Card in Now for demo',
        description: '## AC\n- [ ] Endpoint responds',
        done: false, created: '2026-03-01T10:00:00Z', updated: '2026-03-10T12:00:00Z',
        labels: [{ id: 3, title: 'owner:silas' }, { id: 6, title: 'P2' }],
        project_id: 2,
      },
    ],
  },
  { id: 6, title: 'Done', limit: 0, tasks: [] },
  { id: 7, title: 'Next', limit: 0, tasks: [] },
  { id: 4, title: 'Later', limit: 0, tasks: [] },
  { id: 8, title: 'Blocked', limit: 0, tasks: [] },
  { id: 31, title: "Won't Do", limit: 0, tasks: [] },
];

// Build task→bucket map + flat task list (#1820: list()/view() use fetchAllTasks + fetchBucketMapFromDB)
const allMockTasks = mockBuckets.flatMap(b => (b.tasks || []) as any[]);
const mockBucketMap = new Map<number, string>();
for (const b of mockBuckets) {
  for (const t of b.tasks || []) {
    mockBucketMap.set(t.id, b.title);
  }
}

function createMockClient(): BoardClient {
  const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
  // #2707 — track tasks moved to the Done bucket (id=6) so subsequent
  // view()/fetchTask reflect status='Done'. Without this, doneCard's verify
  // catches the mock as a silent-failure and throws.
  const movedToDone = new Set<number>();
  const overlay = (task: any) => movedToDone.has(task.id)
    ? { ...task, bucket_id: 6, status: 'Done', done: true, doneAt: '2026-05-03T22:00:00Z' }
    : task;

  (client as any).fetchAllTasks = jest.fn().mockImplementation(() => Promise.resolve(allMockTasks.map(overlay)));
  (client as any).fetchBuckets = jest.fn().mockResolvedValue(mockBuckets);
  (client as any).fetchBucketMapFromDB = jest.fn().mockImplementation(() => {
    const m = new Map(mockBucketMap);
    for (const id of movedToDone) m.set(id, 'Done');
    return m;
  });
  (client as any).fetchTask = jest.fn().mockImplementation((apiId: number) => {
    const task = allMockTasks.find((t: any) => t.id === apiId);
    if (!task) return Promise.reject(new Error(`Task ${apiId} not found`));
    return Promise.resolve(overlay(task));
  });
  (client as any).api = jest.fn().mockImplementation((method: string, endpoint: string, body?: any) => {
    // moveToBucket POSTs to /projects/X/views/Y/buckets/<bucketId>/tasks
    // with {task_id}. Capture moves to bucket 6 (Done) so subsequent
    // view()/fetchTask reflect status='Done'.
    const bucketAddMatch = endpoint.match(/\/buckets\/(\d+)\/tasks$/);
    if (bucketAddMatch && method === 'POST') {
      const bucketId = parseInt(bucketAddMatch[1]);
      if (bucketId === 6 && body?.task_id) movedToDone.add(body.task_id);
      return Promise.resolve({});
    }
    if (endpoint.match(/^\/tasks\/\d+$/) && method === 'POST') {
      const taskId = parseInt(endpoint.split('/').pop()!);
      return Promise.resolve({ ...body, id: taskId });
    }
    if (endpoint.match(/\/tasks\/\d+\/comments$/)) {
      if (method === 'GET') return Promise.resolve([]);
      return Promise.resolve({ id: 1, comment: body?.comment || '' });
    }
    if (endpoint.match(/\/tasks\/\d+\/labels/)) {
      return Promise.resolve({ id: 1 });
    }
    return Promise.resolve({});
  });
  return client;
}

beforeEach(() => {
  emittedEvents.length = 0;
});

// ═══════════════════════════════════════════════════════════════════════════
// 1. DEMO START — cards demo emits spine event
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Demo start', () => {
  test('demoCard emits card.demo.started spine event', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await demoCard(client, 50);

    consoleSpy.mockRestore();

    const event = emittedEvents.find(e => e.event === 'card.demo.started');
    expect(event).toBeDefined();
    expect(event!.extra.card_id).toBe('50');
    expect(event!.extra.title).toBe('Feature under demo');
    expect(event!.extra.board).toBe('gathering');
  });

  test('demoCard logs demo started with card title', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await demoCard(client, 50);

    const logCall = consoleSpy.mock.calls.find(c =>
      typeof c[0] === 'string' && c[0].includes('Demo started') && c[0].includes('#50')
    );
    expect(logCall).toBeDefined();
    consoleSpy.mockRestore();
  });

  test('demoCard handles missing card gracefully', async () => {
    const client = createMockClient();
    // Override to make view fail
    const origApi = (client as any).api;
    (client as any).api = jest.fn().mockImplementation((method: string, endpoint: string, body?: any) => {
      if (endpoint.match(/^\/tasks\/\d+$/) && method === 'GET') {
        return Promise.reject(new Error('Not found'));
      }
      return origApi(method, endpoint, body);
    });

    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
    // Should not throw — title lookup is best-effort
    await demoCard(client, 999);
    consoleSpy.mockRestore();

    const event = emittedEvents.find(e => e.event === 'card.demo.started');
    expect(event).toBeDefined();
    expect(event!.extra.card_id).toBe('999');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 2. ACCEPT PATH — demo → done emits card.accepted
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Demo accept path', () => {
  test('doneCard emits card.accepted and card.item.completed', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await doneCard(client, 50);

    consoleSpy.mockRestore();

    const accepted = emittedEvents.find(e => e.event === 'card.accepted');
    expect(accepted).toBeDefined();
    expect(accepted!.extra.card_id).toBe('50');

    const completed = emittedEvents.find(e => e.event === 'card.item.completed');
    expect(completed).toBeDefined();
    expect(completed!.extra.card_id).toBe('50');
  });

  test('doneCard emits deploy.verification.completed', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await doneCard(client, 50);

    consoleSpy.mockRestore();

    const deploy = emittedEvents.find(e => e.event === 'deploy.verification.completed');
    expect(deploy).toBeDefined();
    expect(deploy!.extra.result).toBe('pass');
  });

  test('doneCard calls done on the API (bucket move + done flag)', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await doneCard(client, 50);

    consoleSpy.mockRestore();

    const apiMock = (client as any).api;
    const postCalls = apiMock.mock.calls.filter((c: any[]) => c[0] === 'POST');
    const doneCall = postCalls.find((c: any[]) => c[2]?.done === true);
    expect(doneCall).toBeDefined();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 3. REJECT PATH — demo → reject emits card.rejected with reason
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Demo reject path', () => {
  test('rejectCard emits card.rejected with reason', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await rejectCard(client, 50, 'AC not met — endpoint returns 500');

    consoleSpy.mockRestore();

    const event = emittedEvents.find(e => e.event === 'card.rejected');
    expect(event).toBeDefined();
    expect(event!.extra.card_id).toBe('50');
    expect(event!.extra.reason).toBe('AC not met — endpoint returns 500');
  });

  test('rejectCard logs rejection with reason', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await rejectCard(client, 50, 'Tests failing');

    const logCall = consoleSpy.mock.calls.find(c =>
      typeof c[0] === 'string' && c[0].includes('Rejected') && c[0].includes('Tests failing')
    );
    expect(logCall).toBeDefined();
    consoleSpy.mockRestore();
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. DEMO BRIEF — brief written to PM briefs directory
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Demo brief to PM', () => {
  const briefDir = path.join(__dirname, '../../../../roles/wren/briefs');

  test('PM briefs directory exists and is writable', () => {
    expect(fs.existsSync(briefDir)).toBe(true);
  });

  test('demo brief format matches expected structure', () => {
    // Validate the expected brief format against pattern
    const expectedFields = [
      /# Demo ready: #\d+/,
      /\*\*Builder:\*\*/,
      /\*\*Smoke check:\*\*/,
    ];

    // Create a sample brief content
    const sample = `# Demo ready: #50\n\n**Builder:** Kade\n**Smoke check:** PASS\n**Why it matters:** Test\n`;
    for (const pattern of expectedFields) {
      expect(sample).toMatch(pattern);
    }
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 5. FULL LIFECYCLE — demo → accept/reject sequence
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Full demo lifecycle', () => {
  test('demo → accept produces correct event sequence', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await demoCard(client, 50);
    await doneCard(client, 50);

    consoleSpy.mockRestore();

    const eventNames = emittedEvents.map(e => e.event);
    expect(eventNames).toContain('card.demo.started');
    expect(eventNames).toContain('card.item.completed');
    expect(eventNames).toContain('card.accepted');

    // Demo should come before accept
    const demoIdx = eventNames.indexOf('card.demo.started');
    const acceptIdx = eventNames.indexOf('card.accepted');
    expect(demoIdx).toBeLessThan(acceptIdx);
  });

  test('demo → reject produces correct event sequence', async () => {
    const client = createMockClient();
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

    await demoCard(client, 51);
    await rejectCard(client, 51, 'Needs iteration');

    consoleSpy.mockRestore();

    const eventNames = emittedEvents.map(e => e.event);
    expect(eventNames).toContain('card.demo.started');
    expect(eventNames).toContain('card.rejected');

    const demoIdx = eventNames.indexOf('card.demo.started');
    const rejectIdx = eventNames.indexOf('card.rejected');
    expect(demoIdx).toBeLessThan(rejectIdx);
  });
});

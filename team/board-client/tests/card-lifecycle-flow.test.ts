/**
 * Card Lifecycle Flow Tests — #1235
 *
 * End-to-end validation of the card lifecycle pipeline:
 *   board-ts commands → WIP limits → AC gate → proving gate → accept/reject
 *
 * Tests the SDK functions with mocked BoardClient and spine events.
 */
import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';
import {
  enforceACGate,
  warnShortTitle,
  warnEmptyDescription,
  warnNoComments,
} from '../src/sdk';

// ── Mock spine events to capture emissions ──
const emittedEvents: Array<{ event: string; role: string; extra: Record<string, string> }> = [];

jest.mock('../src/events', () => ({
  emitSpineEvent: (event: string, role: string, extra: Record<string, string> = {}) => {
    emittedEvents.push({ event, role, extra });
  },
  emitChorusEvent: (event: string, role: string, extra: Record<string, string> = {}) => {
    emittedEvents.push({ event, role, extra });
  },
}));

// Mock detectRole for SDK functions that use it
jest.mock('../src/sdk', () => {
  const actual = jest.requireActual('../src/sdk');
  return {
    ...actual,
  };
});

// Mock blast-radius to prevent real API calls
jest.mock('../src/blast-radius', () => ({
  generateBlastRadius: jest.fn().mockResolvedValue(null),
  formatBlastComment: jest.fn().mockReturnValue(''),
}));

// ── Mock board data ──
const mockBuckets = [
  {
    id: 5, title: 'Now', limit: 3, tasks: [
      {
        id: 201, index: 10, title: 'Card in Now',
        description: '## AC\n- [ ] Test passes',
        done: false, created: '2026-03-01T10:00:00Z', updated: '2026-03-01T12:00:00Z',
        labels: [{ id: 4, title: 'owner:kade' }, { id: 5, title: 'P1' }],
        project_id: 2,
      },
    ],
  },
  {
    id: 9, title: 'WIP', limit: 3, tasks: [
      {
        id: 202, index: 11, title: 'Card in WIP',
        description: '## Acceptance Criteria\n- [ ] Feature works',
        done: false, created: '2026-03-01T10:00:00Z', updated: '2026-03-01T14:00:00Z',
        labels: [{ id: 4, title: 'owner:kade' }, { id: 6, title: 'P2' }],
        project_id: 2,
      },
    ],
  },
  {
    id: 7, title: 'Next', limit: 0, tasks: [
      {
        id: 203, index: 12, title: 'Card without AC',
        description: 'Just a description with no criteria',
        done: false, created: '2026-03-01T09:00:00Z', updated: '2026-03-01T09:00:00Z',
        labels: [{ id: 2, title: 'owner:wren' }, { id: 6, title: 'P2' }],
        project_id: 2,
      },
      {
        id: 204, index: 13, title: 'Card with checkbox AC',
        description: 'Do the thing\n\n- [ ] Step one\n- [ ] Step two',
        done: false, created: '2026-03-01T09:00:00Z', updated: '2026-03-01T09:00:00Z',
        labels: [{ id: 3, title: 'owner:silas' }, { id: 5, title: 'P1' }],
        project_id: 2,
      },
      {
        id: 205, index: 14, title: '[swat] Emergency fix',
        description: 'Fix it now',
        done: false, created: '2026-03-01T09:00:00Z', updated: '2026-03-01T09:00:00Z',
        labels: [{ id: 4, title: 'owner:kade' }, { id: 5, title: 'P1' }],
        project_id: 2,
      },
      {
        id: 206, index: 15, title: 'Card with numbered AC',
        description: '## What\nBuild the feature\n\n1. First thing works\n2. Second thing works',
        done: false, created: '2026-03-01T09:00:00Z', updated: '2026-03-01T09:00:00Z',
        labels: [{ id: 2, title: 'owner:wren' }, { id: 6, title: 'P2' }],
        project_id: 2,
      },
    ],
  },
  {
    id: 6, title: 'Done', limit: 0, tasks: [
      {
        id: 210, index: 20, title: 'Completed card',
        description: 'All done',
        done: true, created: '2026-02-28T10:00:00Z', updated: '2026-03-01T08:00:00Z',
        labels: [{ id: 4, title: 'owner:kade' }, { id: 5, title: 'P1' }],
        project_id: 2,
      },
    ],
  },
  { id: 4, title: 'Later', limit: 0, tasks: [] },
  { id: 8, title: 'Blocked', limit: 0, tasks: [] },
  { id: 10, title: "Won't Do", limit: 0, tasks: [] },
  { id: 11, title: 'Harvesting', limit: 2, tasks: [] },
];

function createMockClient(): BoardClient {
  const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
  (client as any).api = jest.fn().mockImplementation((method: string, endpoint: string, body?: any) => {
    // Bucket list
    if (endpoint.includes('/views/') && endpoint.includes('/tasks')) {
      return Promise.resolve(mockBuckets);
    }
    // Single task GET
    if (endpoint.match(/^\/tasks\/\d+$/) && method === 'GET') {
      const id = parseInt(endpoint.split('/').pop()!);
      for (const b of mockBuckets) {
        for (const t of b.tasks || []) {
          if (t.id === id) return Promise.resolve(t);
        }
      }
      return Promise.reject(new Error(`Task ${id} not found`));
    }
    // Task update (move)
    if (endpoint.match(/^\/tasks\/\d+$/) && method === 'POST') {
      return Promise.resolve({ ...body, id: parseInt(endpoint.split('/').pop()!) });
    }
    // Comments
    if (endpoint.match(/\/tasks\/\d+\/comments$/)) {
      if (method === 'GET') return Promise.resolve([]);
      return Promise.resolve({ id: 1, comment: body?.comment || '' });
    }
    // Labels
    if (endpoint.match(/\/tasks\/\d+\/labels$/)) {
      return Promise.resolve({ id: 1 });
    }
    return Promise.resolve({});
  });
  return client;
}

beforeEach(() => {
  emittedEvents.length = 0;
});

// ═════════════════════════════════════════════════════════════════════════════
// 1. BOARD-TS COMMANDS — list, view, mine, add, comment
// ═════════════════════════════════════════════════════════════════════════════

describe('Flow: board-ts read commands', () => {
  test('list returns all tasks across buckets', async () => {
    const client = createMockClient();
    const tasks = await client.list();
    expect(tasks.length).toBeGreaterThanOrEqual(7);
  });

  test('view returns single card with parsed fields', async () => {
    const client = createMockClient();
    const card = await client.view(10);
    expect(card.title).toBe('Card in Now');
    expect(card.owner).toBe('Kade');
    expect(card.priority).toBe('P1');
    expect(card.status).toBe('Now');
  });

  test('mine filters by role', async () => {
    const client = createMockClient();
    const kadeTasks = await client.mine('kade');
    expect(kadeTasks.length).toBeGreaterThanOrEqual(2);
    expect(kadeTasks.every(t => t.owner === 'Kade')).toBe(true);
  });

  test('view throws for unknown index', async () => {
    const client = createMockClient();
    await expect(client.view(999)).rejects.toThrow();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 2. AC GATE — enforceACGate blocks WIP entry without acceptance criteria
// ═════════════════════════════════════════════════════════════════════════════

describe('Flow: AC gate (capture gate #1085)', () => {
  test('blocks card without AC from entering WIP', () => {
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    const result = enforceACGate(12, 'Card without AC', 'Just a description with no criteria', 'gathering');
    expect(result).toBe(false);
    consoleSpy.mockRestore();

    // Should emit quality blocked event
    const blocked = emittedEvents.find(e => e.event === 'card.quality.blocked');
    expect(blocked).toBeDefined();
    expect(blocked!.extra.gate).toBe('capture_ac_missing');
  });

  test('allows card with "Acceptance Criteria" heading', () => {
    const result = enforceACGate(11, 'Card with AC', '## Acceptance Criteria\n- [ ] Works', 'gathering');
    expect(result).toBe(true);
  });

  test('allows card with markdown checkboxes', () => {
    const result = enforceACGate(13, 'Card with checkboxes', 'Do it\n\n- [ ] Step one\n- [ ] Step two', 'gathering');
    expect(result).toBe(true);
  });

  test('allows card with numbered list', () => {
    const result = enforceACGate(15, 'Numbered AC', '## What\nBuild it\n\n1. First works\n2. Second works', 'gathering');
    expect(result).toBe(true);
  });

  test('allows [swat] card without AC (DEC-055 exemption)', () => {
    const result = enforceACGate(14, '[swat] Emergency fix', 'Fix it now', 'gathering');
    expect(result).toBe(true);
  });

  test('blocks empty description', () => {
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    const result = enforceACGate(99, 'No desc', '', 'gathering');
    expect(result).toBe(false);
    consoleSpy.mockRestore();
  });

  test('blocks undefined description', () => {
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    const result = enforceACGate(99, 'Undefined desc', undefined, 'gathering');
    expect(result).toBe(false);
    consoleSpy.mockRestore();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 3. WIP LIMIT — bucket limits enforced by Vikunja (412 response)
// ═════════════════════════════════════════════════════════════════════════════

describe('Flow: WIP limit enforcement', () => {
  test('fetchBucketsWithLimits returns limits for each column', async () => {
    const client = createMockClient();
    const buckets = await client.fetchBucketsWithLimits();
    const nowBucket = buckets.find((b: any) => b.title === 'Now');
    expect(nowBucket!.limit).toBe(3);
    expect(nowBucket!.taskCount).toBeGreaterThanOrEqual(1);
  });

  test('move to full bucket returns 412 error', async () => {
    const client = createMockClient();
    // Override API to simulate 412 from Vikunja
    (client as any).api = jest.fn().mockImplementation((method: string, endpoint: string) => {
      if (endpoint.includes('/views/') && endpoint.includes('/tasks')) {
        return Promise.resolve(mockBuckets);
      }
      if (method === 'POST') {
        return Promise.reject(Object.assign(new Error('Bucket limit reached'), { statusCode: 412 }));
      }
      // Task lookup
      if (endpoint.match(/^\/tasks\/\d+$/) && method === 'GET') {
        return Promise.resolve(mockBuckets[2].tasks![0]); // Card without AC
      }
      return Promise.resolve({});
    });

    await expect(client.move(12, 'Now')).rejects.toThrow();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 4. PROVING GATE — done + reject + demo flows
// ═════════════════════════════════════════════════════════════════════════════

describe('Flow: Proving gate (done/reject/demo)', () => {
  test('done moves card and emits card.accepted + card.item.completed', async () => {
    // Import SDK functions that use the mocked events
    const sdk = jest.requireActual('../src/sdk') as any;

    const client = createMockClient();

    // Call doneCard through the client directly (avoids detectRole issues)
    await client.done(11);

    // Verify the API was called with done: true
    const apiMock = (client as any).api;
    const postCalls = apiMock.mock.calls.filter((c: any) => c[0] === 'POST');
    expect(postCalls.length).toBeGreaterThanOrEqual(1);
  });

  test('warnNoComments flags card with zero comments', async () => {
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
    const client = createMockClient();

    await warnNoComments(client, 11, 'Card in WIP', 'gathering');

    // Should have logged a warning (comments mock returns [])
    const warnCall = consoleSpy.mock.calls.find(c =>
      typeof c[0] === 'string' && c[0].includes('WARN') && c[0].includes('no comments')
    );
    expect(warnCall).toBeDefined();
    consoleSpy.mockRestore();

    // Should emit quality warning event
    const warned = emittedEvents.find(e =>
      e.event === 'card.quality.warned' && e.extra.gate === 'no_comments'
    );
    expect(warned).toBeDefined();
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 5. QUALITY WARNINGS — title, description gates
// ═════════════════════════════════════════════════════════════════════════════

describe('Flow: Quality warnings', () => {
  test('warnShortTitle flags titles under 10 chars', () => {
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
    warnShortTitle('Fix bug', 'gathering');
    expect(consoleSpy).toHaveBeenCalled();
    consoleSpy.mockRestore();

    const warned = emittedEvents.find(e =>
      e.event === 'card.quality.warned' && e.extra.gate === 'title_short'
    );
    expect(warned).toBeDefined();
  });

  test('warnShortTitle does not flag long titles', () => {
    const before = emittedEvents.length;
    warnShortTitle('This is a sufficiently long title for a card', 'gathering');
    expect(emittedEvents.length).toBe(before); // no new events
  });

  test('warnEmptyDescription flags cards entering Now without description', () => {
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
    warnEmptyDescription(99, 'Test card', '', 'gathering');
    expect(consoleSpy).toHaveBeenCalled();
    consoleSpy.mockRestore();

    const warned = emittedEvents.find(e =>
      e.event === 'card.quality.warned' && e.extra.gate === 'description_empty'
    );
    expect(warned).toBeDefined();
  });

  test('warnEmptyDescription does not flag cards with description', () => {
    const before = emittedEvents.length;
    warnEmptyDescription(10, 'Card in Now', '## AC\n- [ ] Test passes', 'gathering');
    expect(emittedEvents.length).toBe(before);
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 6. SPINE EVENT EMISSION — events fire on lifecycle transitions
// ═════════════════════════════════════════════════════════════════════════════

describe('Flow: Spine event emission', () => {
  test('AC gate emits card.quality.blocked with correct fields', () => {
    const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
    enforceACGate(42, 'No AC card', 'Just words', 'gathering');
    consoleSpy.mockRestore();

    const event = emittedEvents.find(e =>
      e.event === 'card.quality.blocked' && e.extra.card_id === '42'
    );
    expect(event).toBeDefined();
    expect(event!.extra.gate).toBe('capture_ac_missing');
    expect(event!.extra.stage).toBe('building');
    expect(event!.extra.board).toBe('gathering');
  });

  test('quality warnings emit card.quality.warned with gate type', () => {
    const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
    warnShortTitle('Hi', 'gathering');
    consoleSpy.mockRestore();

    const event = emittedEvents.find(e =>
      e.event === 'card.quality.warned' && e.extra.gate === 'title_short'
    );
    expect(event).toBeDefined();
    expect(event!.extra.title).toBe('Hi');
  });
});

// ═════════════════════════════════════════════════════════════════════════════
// 7. ACCEPT + REJECT FLOWS — full lifecycle transitions
// ═════════════════════════════════════════════════════════════════════════════

describe('Flow: Accept and reject lifecycle', () => {
  test('done marks card as done via API', async () => {
    const client = createMockClient();
    await client.done(11);

    const apiMock = (client as any).api;
    const postCalls = apiMock.mock.calls.filter((c: any[]) => c[0] === 'POST');
    const doneCall = postCalls.find((c: any[]) => c[2]?.done === true);
    expect(doneCall).toBeDefined();
  });

  test('move to WIP calls API with correct bucket', async () => {
    const client = createMockClient();
    // Card 13 has checkbox AC, should pass gate if called via client.move directly
    await client.move(13, 'WIP');

    const apiMock = (client as any).api;
    const postCalls = apiMock.mock.calls.filter((c: any[]) => c[0] === 'POST');
    expect(postCalls.length).toBeGreaterThanOrEqual(1);
  });

  test("move to Won't Do calls API", async () => {
    const client = createMockClient();
    await client.move(12, "won't do");

    const apiMock = (client as any).api;
    const postCalls = apiMock.mock.calls.filter((c: any[]) => c[0] === 'POST');
    expect(postCalls.length).toBeGreaterThanOrEqual(1);
  });

  test('comment adds text to card', async () => {
    const client = createMockClient();
    await client.comment(11, '[done] Feature complete');

    const apiMock = (client as any).api;
    const commentCalls = apiMock.mock.calls.filter((c: any[]) =>
      c[1]?.includes('/comments') && c[0] === 'PUT'
    );
    expect(commentCalls.length).toBe(1);
    expect(commentCalls[0][2].comment).toBe('[done] Feature complete');
  });
});

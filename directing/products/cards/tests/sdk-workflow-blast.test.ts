/**
 * sdk-workflow-blast.test.ts — #2262
 *
 * Covers two previously untested surfaces in sdk.ts:
 *   1. triggerWorkflow (lines 156-190) — jest.mock of workflow-engine dist
 *   2. generateBlastRadius paths in moveCard (lines 745-803) — jest.mock of ./blast-radius
 */

import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';

// ── Mock workflow-engine (require path from sdk.ts line 66) ──
const mockScanWorkflows = jest.fn();
const mockCreate = jest.fn();
jest.mock('../../../../platform/workflow-engine/dist/engine', () => ({
  WorkflowEngine: jest.fn().mockImplementation(() => ({
    scanWorkflows: mockScanWorkflows,
    create: mockCreate,
  })),
}), { virtual: true });

// ── Mock blast-radius ──
const mockGenerateBlastRadius = jest.fn();
const mockFormatBlastComment = jest.fn();
jest.mock('../src/blast-radius', () => ({
  generateBlastRadius: mockGenerateBlastRadius,
  formatBlastComment: mockFormatBlastComment,
}));

// ── Mock spine events and role detection ──
jest.mock('../src/events', () => ({
  emitSpineEvent: jest.fn(),
  emitChorusEvent: jest.fn(),
}));
jest.mock('../src/config', () => ({
  ...jest.requireActual('../src/config'),
  detectRole: jest.fn().mockReturnValue('kade'),
  autoRoleState: jest.fn(),
}));

import { triggerWorkflow, moveCard } from '../src/sdk';

// Description that passes AC gate (- [ ] checkbox) + Experience gate (## Experience heading)
const VALID_DESC = '## Experience\nJeff sees the feature working.\n\n## AC\n- [ ] It works\n';

function makeClient(overrides: { title?: string; description?: string; domains?: string[]; owner?: string } = {}): BoardClient {
  const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
  jest.spyOn(client, 'view').mockResolvedValue({
    index: 42,
    apiId: 42,
    id: 42,
    title: overrides.title ?? 'Test card for #2262',
    description: overrides.description ?? VALID_DESC,
    owner: overrides.owner ?? 'kade',
    domains: overrides.domains ?? [],
    status: 'WIP',
    priority: 'P2',
    done: false,
    created: '2026-01-01',
    updated: '2026-01-01',
    comments: [],
  } as any);
  jest.spyOn(client, 'move').mockResolvedValue(undefined as any);
  jest.spyOn(client, 'comment').mockResolvedValue(undefined as any);
  Object.defineProperty(client, 'boardName', { get: () => 'gathering' });
  return client;
}

beforeEach(() => {
  jest.clearAllMocks();
});

// ── AC1: triggerWorkflow ──

describe('triggerWorkflow', () => {
  it('creates a workflow manifest when none exists for the card', async () => {
    mockScanWorkflows.mockReturnValue([]);
    mockCreate.mockReturnValue({ id: 'WF-001' });

    await triggerWorkflow(makeClient(), 42);

    expect(mockCreate).toHaveBeenCalledTimes(1);
    const [title, steps] = mockCreate.mock.calls[0];
    expect(title).toBe('Test card for #2262');
    expect(steps).toMatch(/kade/);
  });

  it('skips creation when workflow already exists for this card', async () => {
    mockScanWorkflows.mockReturnValue([{ id: 'WF-existing', card: 42 }]);

    await triggerWorkflow(makeClient(), 42);

    expect(mockCreate).not.toHaveBeenCalled();
  });

  it('routes wren-owned cards: builder=kade reviewer=silas', async () => {
    mockScanWorkflows.mockReturnValue([]);
    mockCreate.mockReturnValue({ id: 'WF-002' });

    await triggerWorkflow(makeClient({ owner: 'wren' }), 42);

    const [, steps] = mockCreate.mock.calls[0];
    expect(steps).toMatch(/kade/);
    expect(steps).toMatch(/silas/);
  });

  it('uses C# prefix for chorus-domain cards', async () => {
    mockScanWorkflows.mockReturnValue([]);
    mockCreate.mockReturnValue({ id: 'WF-003' });

    await triggerWorkflow(makeClient({ domains: ['product:chorus'] }), 42);

    const [, steps] = mockCreate.mock.calls[0];
    expect(steps).toMatch(/C#42/);
  });
});

// ── AC2: generateBlastRadius paths in moveCard ──

describe('moveCard blast-radius paths', () => {
  it('posts a blast-radius comment when totalFiles > 0', async () => {
    mockGenerateBlastRadius.mockResolvedValue({
      totalFiles: 8,
      crossDomain: ['chorus', 'gathering'],
    });
    mockFormatBlastComment.mockReturnValue('**Blast Radius** — 8 files, 2 domains');

    // Description includes AC+Experience (passes WIP gates) + code keyword (triggers isCodeCard)
    const client = makeClient({
      description: VALID_DESC + 'fix the route handler in handlers/foo.ts',
    });
    await moveCard(client, 42, 'WIP');

    expect(mockGenerateBlastRadius).toHaveBeenCalled();
    expect(client.comment).toHaveBeenCalledWith(42, expect.stringMatching(/Blast Radius/));
  });

  it('skips blast-radius comment when totalFiles = 0', async () => {
    mockGenerateBlastRadius.mockResolvedValue({ totalFiles: 0, crossDomain: [] });
    mockFormatBlastComment.mockReturnValue('');

    // [process] title + non-code description → isCodeCard returns false → no blast-radius
    const client = makeClient({
      title: '[process] update team docs',
      description: VALID_DESC + 'write a planning brief',
    });
    await moveCard(client, 42, 'WIP');

    const blastComments = (client.comment as jest.Mock).mock.calls.filter(
      ([, msg]: [number, string]) => typeof msg === 'string' && msg.includes('Blast Radius'),
    );
    expect(blastComments).toHaveLength(0);
  });

  it('does not throw when generateBlastRadius rejects', async () => {
    mockGenerateBlastRadius.mockRejectedValue(new Error('API down'));

    const client = makeClient();
    await expect(moveCard(client, 42, 'WIP')).resolves.not.toThrow();
  });
});

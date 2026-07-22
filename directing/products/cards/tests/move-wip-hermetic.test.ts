// @test-type: unit — moveCard to WIP must be hermetic: with blast-radius
// mocked at the module seam (the suite's sanctioned network boundary), no
// other code in the move flow may fetch. Pre-#3663 the permutation suite
// skipped this mock, generateBlastRadius fetched live :3000/:3340
// (AbortSignal.timeout(5000) per call), and coverage instrumentation pushed
// it past the jest budget — nightly graded coverage:cards rc=1.

jest.mock('../src/blast-radius', () => ({
  generateBlastRadius: jest.fn(),
  formatBlastComment: jest.fn(),
}));

import { moveCard } from '../src/sdk';
import type { BoardClient } from '../src/client';

function minimalClient(recorded: string[]): BoardClient {
  const task = {
    index: 42, apiId: 420, title: 'hermetic move fixture',
    description: '## Experience\nSees it.\n\n## AC\n- [ ] item one',
    status: 'Next', owner: 'Kade', priority: 'P2',
    domains: ['domain:chorus', 'type:fix', 'sequence:werk', 'chunk:tests', 'origin:reactive'],
  };
  return {
    boardName: 'gathering',
    view: async () => task,
    move: async () => { recorded.push('move'); },
    comment: async () => { recorded.push('comment'); },
    comments: async () => [],
    listGrouped: async () => new Map([['WIP', []]]),
  } as unknown as BoardClient;
}

describe('moveCard → WIP is hermetic (#3663)', () => {
  test('no fetch leaves the process during a WIP move under NODE_ENV=test', async () => {
    const fetchCalls: string[] = [];
    const origFetch = globalThis.fetch;
    (globalThis as { fetch: unknown }).fetch = async (url: unknown) => {
      fetchCalls.push(String(url));
      return { ok: false, status: 599, json: async () => null, text: async () => '' };
    };
    const origLog = console.log;
    const origErr = console.error;
    console.log = () => {};
    console.error = () => {};
    try {
      await moveCard(minimalClient([]), 42, 'WIP');
    } finally {
      (globalThis as { fetch: unknown }).fetch = origFetch;
      console.log = origLog;
      console.error = origErr;
    }

    expect(fetchCalls).toEqual([]);
  });
});

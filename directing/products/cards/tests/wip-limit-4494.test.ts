// @test-type: unit
// @domain: cards — placed by chorus-crawl's domain rules (#4538)
/**
 * #4494 — Jeff, 2026-10-11 09:06: hard WIP 3 on the board (two card runs, one
 * land). The cards CLI refuses a fourth card into WIP.
 */
jest.mock('../src/blast-radius', () => ({ generateBlastRadius: jest.fn(), formatBlastComment: jest.fn() }));

import { moveCard, wipLimitRefusal, WIP_LIMIT } from '../src/sdk';
import type { BoardClient } from '../src/client';

function client(wip: number[], moved: number[]): BoardClient {
  const task = (index: number) => ({
    index, apiId: index * 10, title: `fixture ${index}`,
    description: '## Experience\nSees it.\n\n## AC\n- [ ] item one',
    status: 'Next', owner: 'Wren', priority: 'P2',
    domains: ['domain:chorus', 'type:fix', 'sequence:werk', 'chunk:tests', 'origin:reactive'],
  });
  return {
    boardName: 'gathering',
    view: async (i: number) => task(i),
    move: async (i: number) => { moved.push(i); },
    comment: async () => {},
    comments: async () => [],
    listGrouped: async () => new Map([['WIP', wip.map(task)]]),
  } as unknown as BoardClient;
}

describe('#4494 hard WIP limit', () => {
  it('is three', () => expect(WIP_LIMIT).toBe(3));

  it('lets a card in while WIP has room', () => {
    expect(wipLimitRefusal([1, 2], 9)).toBeNull();
  });

  it('refuses a fourth card and names the three holding WIP', () => {
    expect(wipLimitRefusal([1, 2, 3], 9)).toBe('ERROR: WIP is full: 3 of 3 (#1, #2, #3). Land or unpull one before #9 enters WIP.');
  });

  it('lets a card already in WIP move to WIP again', () => {
    expect(wipLimitRefusal([1, 2, 3], 2)).toBeNull();
  });

  // NEGATIVE PROOF: the move itself is stopped, not only the message. process.exit
  // ends the CLI for real; here it is recorded, so the order shows the exit came first.
  it('NEGATIVE PROOF — moveCard to WIP with three in WIP exits 1 before any move', async () => {
    const order: string[] = [];
    const moved: number[] = [];
    const c = client([1, 2, 3], moved);
    const move = c.move;
    (c as { move: unknown }).move = async (i: number) => { order.push('move'); return move(i, 'WIP'); };
    jest.spyOn(process, 'exit').mockImplementation(((code?: number) => { order.push(`exit ${code}`); throw new Error('exit'); }) as never);
    const err = jest.spyOn(console, 'error').mockImplementation(() => {});
    jest.spyOn(console, 'log').mockImplementation(() => {});
    await moveCard(c, 9, 'WIP').catch(() => {});
    expect(order[0]).toBe('exit 1');
    expect(err.mock.calls.flat().join('\n')).toContain('WIP is full');
    jest.restoreAllMocks();
  });

  it('with room, the same move goes through', async () => {
    const moved: number[] = [];
    jest.spyOn(console, 'log').mockImplementation(() => {});
    jest.spyOn(console, 'error').mockImplementation(() => {});
    await moveCard(client([1, 2], moved), 9, 'WIP');
    expect(moved).toEqual([9]);
    jest.restoreAllMocks();
  });
});

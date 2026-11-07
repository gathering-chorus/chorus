// @test-type: unit — a stand-in board client; no live board, no spine.
// @domain: cards
// @card: #4660
// @owner: wren
/**
 * #4660 — a demo never ticks AC boxes. Measured 10-20 on #4652: Jeff's two after-land
 * checks were ticked by `cards demo` at 07:24 and again near 08:34, and the product gate
 * passed on them. demoCard replaced every "- [ ]" with "- [x]" (#2017, 04-14).
 */
import { demoCard } from '../src/sdk';
import type { BoardClient } from '../src/client';

jest.mock('../src/events', () => ({ emitSpineEvent: () => undefined, emitChorusEvent: () => undefined }));
jest.mock('../src/config', () => ({ ...jest.requireActual('../src/config'), detectRole: () => 'wren' }));

const DESC = '## AC\n- [x] built and shown\n- [ ] After the land: Jeff checks it\n';

function board(description: string) {
  const updates: Array<Record<string, unknown>> = [];
  const client = {
    boardName: 'chorus',
    view: async () => ({ title: 'a card', description }),
    update: async (_i: number, fields: Record<string, unknown>) => { updates.push(fields); },
  } as unknown as BoardClient;
  return { client, updates };
}

describe('#4660 a demo leaves the AC as the owner left it', () => {
  test('NEGATIVE PROOF: an unticked after-land box is still unticked after a demo, and nothing is written', async () => {
    const { client, updates } = board(DESC);
    const log = jest.spyOn(console, 'log').mockImplementation(() => undefined);
    await demoCard(client, 4652);
    log.mockRestore();
    expect(updates).toEqual([]);
  });

  test('a card whose boxes are all ticked is also left alone', async () => {
    const { client, updates } = board('## AC\n- [x] one\n- [x] two\n');
    const log = jest.spyOn(console, 'log').mockImplementation(() => undefined);
    await demoCard(client, 1);
    log.mockRestore();
    expect(updates).toEqual([]);
  });
});

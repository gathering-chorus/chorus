// @test-type: unit — a stubbed board API in-process; no Vikunja, no network.
/**
 * #4457 — Jeff, 2026-10-08 12:34: "yes fix". Abby pulled #4456 and the board
 * could not name her: owners were typed into config.ts as jeff/wren/silas/kade.
 * Owners are now the board's own owner:* labels, so a role becomes an owner when
 * the board has its label, and nothing in code lists the team.
 */
import { BoardClient } from '../src/client';
import { GATHERING } from '../src/config';

type Label = { id: number; title: string };

function board(labels: Label[]) {
  const client = new BoardClient('http://localhost:3456', 'fake-token', GATHERING);
  const c = client as unknown as Record<string, unknown>;
  const added: number[] = [];
  const removed: number[] = [];
  c.resolveIndex = jest.fn(async (i: number) => 1000 + i);
  c.fetchTask = jest.fn(async () => ({ labels: [{ id: 2, title: 'owner:wren' }] }));
  c.listLabels = jest.fn(async () => labels);
  c.addLabel = jest.fn(async (_a: number, l: number) => { added.push(l); });
  c.removeLabel = jest.fn(async (_a: number, l: number) => { removed.push(l); });
  c.syncToGraph = jest.fn(async () => undefined);
  return { client, added, removed };
}

const TEAM: Label[] = [
  { id: 1, title: 'owner:jeff' }, { id: 2, title: 'owner:wren' },
  { id: 3, title: 'owner:silas' }, { id: 4, title: 'owner:kade' },
  { id: 900, title: 'owner:abby-normal' }, { id: 50, title: 'chunk:werk' },
];

describe('#4457 owners are the board\'s owner:* labels', () => {
  test('a card can be given to abby-normal once the board has her label', async () => {
    const b = board(TEAM);
    const r = await b.client.reassignOwner(4456, 'abby-normal');
    expect(r.oldOwner).toBe('wren');
    expect(b.removed).toEqual([2]);
    expect(b.added).toEqual([900]);
  });

  test('NEGATIVE PROOF: without her label she is refused by name, and the card keeps its owner', async () => {
    const b = board(TEAM.filter((l) => l.title !== 'owner:abby-normal'));
    await expect(b.client.reassignOwner(4456, 'abby-normal'))
      .rejects.toThrow('Unknown owner "abby-normal". Valid: jeff, wren, silas, kade');
    expect(b.removed).toEqual([]);
    expect(b.added).toEqual([]);
  });

  test('the valid list names only owner labels, never other kinds', async () => {
    const b = board([{ id: 4, title: 'owner:kade' }, { id: 50, title: 'chunk:werk' }]);
    await expect(b.client.ownerLabelId('werk')).rejects.toThrow(/Valid: kade$/);
  });
});

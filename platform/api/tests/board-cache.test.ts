import { parseCardsListOutput, createBoardCache, CachedCard } from '../src/board-cache';

describe('parseCardsListOutput', () => {
  it('parses a single status section with one card', () => {
    const stdout = `WIP (1):
  2205  Test card [Kade|P2|chunk:ops|domain:chorus|type:enhance]
`;
    const result = parseCardsListOutput(stdout);
    expect(result).toHaveLength(1);
    expect(result[0]).toEqual({
      id: '2205',
      title: 'Test card',
      status: 'WIP',
      owner: 'kade',
      type: 'enhance',
      priority: 'P2',
      domain: 'chorus', // #3149-fix — parsed from the domain:chorus tag
      tags: 'Kade|P2|chunk:ops|domain:chorus|type:enhance',
    });
  });

  it('parses multiple status sections', () => {
    const stdout = `WIP (1):
  100  A [P1|type:fix]

Next (2):
  200  B [Wren|P2|type:new]
  300  C [Silas|P3|type:chore]
`;
    const result = parseCardsListOutput(stdout);
    expect(result).toHaveLength(3);
    expect(result.map(c => c.status)).toEqual(['WIP', 'Next', 'Next']);
    expect(result.map(c => c.id)).toEqual(['100', '200', '300']);
  });

  it("recognizes Won't Do as a status", () => {
    const result = parseCardsListOutput(`Won't Do (1):\n  99  Rejected card [Kade|P2]\n`);
    expect(result[0].status).toBe("Won't Do");
  });

  it('lowercases the owner name from the tag list', () => {
    const result = parseCardsListOutput(`WIP (1):\n  1  A [WREN|P1]\n`);
    expect(result[0].owner).toBe('wren');
  });

  it('leaves owner empty when no role prefix present', () => {
    const result = parseCardsListOutput(`WIP (1):\n  1  Card without owner [P1|type:fix]\n`);
    expect(result[0].owner).toBe('');
  });

  it('extracts priority with the P prefix', () => {
    const result = parseCardsListOutput(`WIP (1):\n  1  A [P2|type:fix]\n`);
    expect(result[0].priority).toBe('P2');
  });

  it('leaves priority empty when not present', () => {
    const result = parseCardsListOutput(`WIP (1):\n  1  A [type:fix]\n`);
    expect(result[0].priority).toBe('');
  });

  it('extracts type from type: tag', () => {
    const result = parseCardsListOutput(`WIP (1):\n  1  A [P2|type:enhance]\n`);
    expect(result[0].type).toBe('enhance');
  });

  it('trims whitespace from the title', () => {
    const result = parseCardsListOutput(`WIP (1):\n  42    Title with padding    [P1|type:fix]\n`);
    expect(result[0].title).toBe('Title with padding');
  });

  it('ignores lines that are not card rows', () => {
    const stdout = `Header line
WIP (1):
  some non-card line
  42  Real card [P1]

Blank section line

Next (0):
`;
    const result = parseCardsListOutput(stdout);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe('42');
  });

  it('returns empty array on empty output', () => {
    expect(parseCardsListOutput('')).toEqual([]);
  });
});

describe('createBoardCache', () => {
  it('returns empty list before first refresh', () => {
    const cache = createBoardCache({ run: jest.fn(async () => '') });
    // getCards on an empty cache also triggers a background refresh; the
    // synchronously-returned snapshot is still empty.
    const snapshot: CachedCard[] = cache.getCards();
    expect(snapshot).toEqual([]);
  });

  it('populates cards after successful refresh', async () => {
    const run = jest.fn(async () => 'WIP (1):\n  42  Card [Kade|P1|type:fix]\n');
    const cache = createBoardCache({ run });
    await cache.refresh();
    const cards = cache.getCards();
    expect(cards).toHaveLength(1);
    expect(cards[0].id).toBe('42');
  });

  it('keeps previous cards when refresh fails', async () => {
    let call = 0;
    const run = jest.fn(async () => {
      call++;
      if (call === 1) return 'WIP (1):\n  1  OK [P1]\n';
      throw new Error('command died');
    });
    const cache = createBoardCache({ run });
    await cache.refresh();
    expect(cache.getCards()).toHaveLength(1);
    await cache.refresh();
    expect(cache.getCards()).toHaveLength(1);
    expect(cache.getCards()[0].id).toBe('1');
  });

  it('getCards triggers a background refresh when cache is unpopulated', () => {
    const run = jest.fn(async () => 'WIP (1):\n  1  C [P1]\n');
    const cache = createBoardCache({ run });
    cache.getCards();
    expect(run).toHaveBeenCalledTimes(1);
  });

  it('getCards does NOT fire a second refresh once cache is populated', async () => {
    const run = jest.fn(async () => 'WIP (1):\n  1  C [P1]\n');
    const cache = createBoardCache({ run });
    await cache.refresh();
    cache.getCards();
    cache.getCards();
    expect(run).toHaveBeenCalledTimes(1);
  });

  it('exposes the age of the most recent successful refresh', async () => {
    const run = jest.fn(async () => 'WIP (1):\n  1  C [P1]\n');
    const cache = createBoardCache({ run });
    expect(cache.ageMs()).toBe(0);
    const before = Date.now();
    await cache.refresh();
    const after = Date.now();
    const age = cache.ageMs();
    expect(age).toBeGreaterThanOrEqual(before);
    expect(age).toBeLessThanOrEqual(after);
  });
});

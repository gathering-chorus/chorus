/**
 * tiles.ts — API board fetch (#2261).
 *
 * Jeff sees WIP cards in role tiles. TilePoller.refreshBoardFromApi fetches
 * from the Context API. The API wraps cards under data.data.cards — if the
 * fetch parses the wrong path, tiles show no cards even when the API is healthy.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'tiles-api-test-'));
process.env.CLEARING_SCAN_DIR = TMP;
process.env.CLEARING_PULSE_FILE = path.join(TMP, 'pulse.json');

// Write a minimal pulse file so TilePoller doesn't throw on divergence read
fs.writeFileSync(path.join(TMP, 'pulse.json'), JSON.stringify({ board: { wip_cards: [], swat_cards: [] }, roles: {} }));

// Stub global fetch before importing TilePoller
const mockFetch = jest.fn();
(global as unknown as { fetch: unknown }).fetch = mockFetch;

import { TilePoller } from '../src/tiles';

function apiResponse(cards: Array<{ id: number; owner: string; domain: string }>) {
  return Promise.resolve({
    ok: true,
    json: () => Promise.resolve({
      domain: 'chorus',
      timestamp: new Date().toISOString(),
      source: '/api/chorus/context/board/wip',
      data: { total: cards.length, cards },
    }),
  } as Response);
}

describe('TilePoller — API board fetch (#2261)', () => {
  beforeEach(() => mockFetch.mockReset());

  it('tiles show WIP card for Wren when API returns data.data.cards', async () => {
    mockFetch.mockReturnValue(apiResponse([{ id: 2261, owner: 'Wren', domain: 'chorus' }]));

    const poller = new TilePoller();
    // Allow the async fetch to resolve
    await new Promise((r) => setTimeout(r, 50));
    poller.poll();
    await new Promise((r) => setTimeout(r, 50));

    const tiles = poller.getTiles();
    const wren = tiles.find((t) => t.role === 'wren');
    expect(wren?.cards).toContain('#2261');
  });

  it('tiles show SWAT card for Wren labeled [swat] when swat API returns data.data.cards', async () => {
    mockFetch.mockImplementation((url: string) =>
      url.includes('/swat')
        ? apiResponse([{ id: 2263, owner: 'Wren', domain: 'chorus' }])
        : apiResponse([]),
    );

    const poller = new TilePoller();
    await new Promise((r) => setTimeout(r, 50));
    poller.poll();
    await new Promise((r) => setTimeout(r, 50));

    const tiles = poller.getTiles();
    const wren = tiles.find((t) => t.role === 'wren');
    expect(wren?.cards ?? []).toContain('#2263[swat]');
  });
});

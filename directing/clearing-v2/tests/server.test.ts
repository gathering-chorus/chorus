// @test-type: unit — verifies clearing-v2 server route mounting and tile-service state, no live network services.
// @card: #4530
// @owner: abby-normal
// @domain: messages

import { createApp } from '../src/server';
import { TileService } from '../src/tile-service';

describe('Clearing v2: Server App & Tile Service', () => {
  test('createApp initializes app, router, relay and tileService', () => {
    const { app, router, tileService, relay } = createApp({
      relayUrl: 'ws://127.0.0.1:3489',
      pulseUrl: 'http://127.0.0.1:3475',
    });
    expect(app).toBeDefined();
    expect(router).toBeDefined();
    expect(tileService).toBeDefined();
    expect(relay).toBeDefined();
  });

  test('TileService initializes known team roles with idle state', () => {
    const tileService = new TileService('http://127.0.0.1:3475');
    const tiles = tileService.getTiles();
    expect(tiles.length).toBe(5);
    const roles = tiles.map(t => t.role);
    expect(roles).toContain('abby-normal');
    expect(roles).toContain('wren');
    expect(roles).toContain('silas');
    expect(roles).toContain('kade');
    expect(roles).toContain('jeff');
    const abbyTile = tiles.find(t => t.role === 'abby-normal');
    expect(abbyTile?.name).toBe('Abby Normal');
    expect(abbyTile?.state).toBe('idle');
  });

  test('TileService updates tile state incrementally', () => {
    const tileService = new TileService();
    tileService.updateTile('abby-normal', { state: 'building', card: 4530 });
    const tiles = tileService.getTiles();
    const abbyTile = tiles.find(t => t.role === 'abby-normal');
    expect(abbyTile?.state).toBe('building');
    expect(abbyTile?.card).toBe(4530);
  });

  test('NEGATIVE PROOF: updating nonexistent role does not corrupt tile list', () => {
    const tileService = new TileService();
    tileService.updateTile('ghost-agent', { state: 'idle' });
    const tiles = tileService.getTiles();
    expect(tiles.length).toBe(5);
    expect(tiles.find(t => t.role === 'ghost-agent')).toBeUndefined();
  });
});

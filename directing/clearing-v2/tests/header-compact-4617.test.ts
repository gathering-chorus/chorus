// @test-type: unit — Clearing v2 header compaction (#4617): state chips in title row, drop actor rail on Rooms, room picker dropdown in header.
// @card: #4617
// @owner: abby-normal
// @domain: messages

import { renderPageShell, renderRoomsPage } from '../src/views';
import { RoomsData, TrustStripSource } from '../src/sources/models';

describe('#4617 Clearing v2 header: state chips beside title, no actor row, rooms as a picker', () => {
  const mockSources: TrustStripSource[] = [
    { source: 'rooms', status: 'ok', asOf: 'connected' },
    { source: 'board', status: 'ok', asOf: 'as of 14:00' },
    { source: 'decisions', status: 'unmeasured', reason: 'no source, part 2' },
    { source: 'roles', status: 'ok', asOf: 'as of 13:30', lagMinutes: 30 },
    { source: 'loki', status: 'ok', asOf: 'as of 14:00' },
  ];

  const mockRoomsData: RoomsData = {
    room: 'team',
    channels: ['team', 'gathering', 'chorus'],
    directRoles: ['Wren', 'Silas', 'Kade', 'Abby Normal'],
    messages: [],
    relayConnected: true,
  };

  test('AC1: State chips render in .app-header beside title; trust-strip row below nav is removed', () => {
    const html = renderPageShell('Rooms', 'rooms', '<div id="test-body">Body</div>', mockSources);

    // Header has the trust dots beside title/relay status
    expect(html).toContain('class="app-header"');
    expect(html).toMatch(/<header class="app-header">[\s\S]*class="strip trust-strip"[\s\S]*<\/header>/);
    expect(html).toContain('board · as of 14:00');
    expect(html).toContain('decisions');
    expect(html).toContain('strip-source-fail');

    // Negative proof: No separate trust-strip row below the nav
    expect(html).not.toMatch(/<\/nav>\s*<div class="strip trust-strip">/);
  });

  test('AC2: Rooms page drops the actor-card rail (Teammates has it)', () => {
    const html = renderRoomsPage('team', mockRoomsData, mockSources);

    // Negative proof: actor rail is NOT rendered on Rooms
    expect(html).not.toContain('id="rooms-actor-rail"');
    expect(html).not.toContain('class="actor-rail"');
  });

  test('AC3: Rooms page drops the rooms/direct row and replaces it with #team picker in the chat header', () => {
    const html = renderRoomsPage('team', mockRoomsData, mockSources);

    // Negative proof: separate inline nav bar is NOT rendered
    expect(html).not.toContain('id="rooms-nav-bar"');
    expect(html).not.toContain('class="rooms-nav-inline"');

    // Positive proof: room picker dropdown is in the chat header
    expect(html).toContain('id="room-picker"');
    expect(html).toContain('<option value="team" selected># team</option>');
    expect(html).toContain('<option value="wren">@ Wren</option>');
    expect(html).toContain('<option value="silas">@ Silas</option>');
    expect(html).toContain('<option value="kade">@ Kade</option>');
    expect(html).toContain('<option value="abby-normal">@ Abby Normal</option>');
  });
});

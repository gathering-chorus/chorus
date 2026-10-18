// @test-type: unit — Clearing v2 folded ACs (#4597): picker keeps base, one people row with lights/cards, top bar status dots, real card drawer with comments and stale cws.
// @card: #4597
// @owner: abby-normal
// @domain: messages

import fs from 'fs';
import path from 'path';
import { renderPageShell, renderRoomsPage } from '../src/views';
import { toDrawerCard } from '../src/sources/card';
import { fetchCardPipeline } from '../src/sources/pipeline';
import { RoomsData, TrustStripSource } from '../src/sources/models';

describe('#4597 Clearing v2: folded ACs (picker base, one people row, status dots, card drawer)', () => {
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
    actors: [
      { id: 'wren', name: 'Wren', state: 'building', activity: 'acting', sessions: ['live'], cards: [{ id: 4590, title: 'Wren Card' }] },
      { id: 'silas', name: 'Silas', state: 'available', activity: 'idle', sessions: [], cards: [] },
      { id: 'kade', name: 'Kade', state: 'waiting', activity: 'waiting', sessions: [], card: { id: 4595, title: 'Kade Card' } },
      { id: 'abby-normal', name: 'Abby Normal', state: 'building', activity: 'acting', sessions: ['live'], card: { id: 4597, title: 'Abby Card' } },
    ],
  };

  test('AC6: Room picker keeps /clearing-v2 base path on change', () => {
    const html = renderRoomsPage('team', mockRoomsData, mockSources);

    // Picker has onchange that prepends window.CLEARING_BASE
    expect(html).toContain('id="room-picker"');
    expect(html).toContain("window.location.href=(window.CLEARING_BASE || '') + '/rooms/' + this.value");
    expect(html).toContain('<option value="team" selected># team</option>');
    expect(html).toContain('<option value="wren">@ Wren</option>');
    expect(html).toContain('<option value="silas">@ Silas</option>');
    expect(html).toContain('<option value="kade">@ Kade</option>');
    expect(html).toContain('<option value="abby-normal">@ Abby Normal</option>');
  });

  test('AC7: One people row on Rooms (#team + roles with lights and cards); tapping opens direct room; no separate DIRECT row', () => {
    const html = renderRoomsPage('team', mockRoomsData, mockSources);

    // Positive proof: One people row is rendered with #team tile and role tiles
    expect(html).toContain('id="rooms-people-row"');
    expect(html).toContain('team-tile');
    expect(html).toContain("window.location.href=(window.CLEARING_BASE || '') + '/rooms/team'");

    // Role tiles with andon lights and current cards
    expect(html).toContain('actor-tile-wren');
    expect(html).toContain('actor-tile-silas');
    expect(html).toContain('actor-tile-kade');
    expect(html).toContain('actor-tile-abby-normal');
    expect(html).toContain('andon-dot');
    expect(html).toContain('#4590');
    expect(html).toContain('#4597');

    // Tapping a tile navigates to that person's direct room
    expect(html).toContain("window.location.href=(window.CLEARING_BASE || '') + '/rooms/wren'");
    expect(html).toContain("window.location.href=(window.CLEARING_BASE || '') + '/rooms/abby-normal'");

    // Negative proof: NO separate DIRECT row
    expect(html).not.toContain('id="rooms-nav-bar"');
    expect(html).not.toContain('class="rooms-nav-inline"');
  });

  test('AC8: Status dots sit in the top bar to the right of "The Clearing v2" with tooltips and no word clutter', () => {
    const html = renderPageShell('Rooms', 'rooms', '<div id="test-body">Body</div>', mockSources);

    // Header has the trust dots beside title/relay status
    expect(html).toContain('class="app-header"');
    expect(html).toMatch(/<header class="app-header">[\s\S]*class="strip trust-strip"[\s\S]*<\/header>/);
    expect(html).toContain('board · as of 14:00');
    expect(html).toContain('strip-source-fail');

    // Negative proof: No separate trust-strip row below the nav
    expect(html).not.toMatch(/<\/nav>\s*<div class="strip trust-strip">/);

    // CSS rule hides word clutter on narrow width (mobile), keeping dots clean
    const cssPath = path.resolve(__dirname, '../public/style.css');
    const css = fs.readFileSync(cssPath, 'utf8');
    expect(css).toMatch(/\.app-header \.trust-strip \.chip\s*\{[^}]*font-size:\s*0/);
    expect(css).toMatch(/\.app-header \.trust-strip \.chip \.fail-source-name\s*\{[^}]*display:\s*none/);
  });

  test('AC9: Card drawer shows real card (status WIP, description, AC ticks, last 3 comments); /cws keeps last good marked stale on timeout', async () => {
    // 1. toDrawerCard extracts status WIP, description, AC items, and last 3 comments
    const cardData = {
      kind: 'Card',
      data: {
        title: 'Clearing v2 formatting',
        status: 'WIP',
        owner: 'Abby-normal',
        description: '## Why this matters\nMessage formatting\n\n## AC\n- [ ] Line breaks\n- [x] Code fences',
        ac_items: [
          { text: 'Line breaks', checked: false },
          { text: 'Code fences', checked: true },
        ],
        comments: [
          { author: 'silas', text: 'comment 1' },
          { author: 'wren', text: 'comment 2' },
          { author: 'jeff', text: 'comment 3' },
          { author: 'kade', text: 'comment 4' },
        ],
      },
    };
    const drawerCard = toDrawerCard(4597, cardData);
    expect(drawerCard).not.toBeNull();
    expect(drawerCard?.status).toBe('WIP');
    expect(drawerCard?.description).toContain('Message formatting');
    expect(drawerCard?.acItems?.length).toBe(2);
    expect(drawerCard?.acItems?.[1].checked).toBe(true);
    // Extracts last 3 comments only
    expect(drawerCard?.comments?.length).toBe(3);
    expect(drawerCard?.comments?.[0].author).toBe('wren');
    expect(drawerCard?.comments?.[2].author).toBe('kade');

    // 2. /cws retains last good output marked stale on timeout
    const goodOutput = '#4597 abby-normal running (16:00) · cicd run 1\n  step: skill-werk-test';
    const res1 = await fetchCardPipeline(4597, {
      runCws: async () => goodOutput,
    });
    expect(res1.status).toBe('running');

    const resTimeout = await fetchCardPipeline(4597, {
      runCws: async () => {
        throw new Error('Timed out after 10000ms');
      },
    });
    expect(resTimeout.isStale).toBe(true);
    expect(resTimeout.output).toContain(goodOutput);
    expect(resTimeout.output).toContain('showing last good output');

    // 3. Rooms card drawer contains comments container
    const roomsHtml = renderRoomsPage('team', mockRoomsData, mockSources);
    expect(roomsHtml).toContain('id="drawer-card-comments-container"');
    expect(roomsHtml).toContain('id="drawer-card-comments"');
    expect(roomsHtml).toContain("var staleLabel = (p.isStale || p.status === 'stale') ? 'stale' : 'unmeasured'");
  });
});

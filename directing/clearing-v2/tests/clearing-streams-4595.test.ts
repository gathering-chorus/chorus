// @test-type: unit — Clearing v2 streams, actor rail, andon lights, and 0..n WIP cards (#4595)
// @domain: messages
// @card: #4595
// @owner: abby-normal

import { renderRoomsPage, renderBoardPage, renderRolesPage, roleAndonLight, ownerPresence } from '../src/views';
import { toActorRow } from '../src/sources/pulse';
import { BoardColumn } from '../src/sources/board';
import { RoomsData, RolesData, PulseActor } from '../src/sources/models';

describe('#4595 Clearing v2 live streams, actor rail, and idle/busy lights with motion', () => {
  const mockRolesData: RolesData = {
    actors: [
      {
        id: 'abby-normal',
        name: 'Abby Normal',
        state: 'building',
        activity: 'acting',
        card: { id: 4591, title: 'Message backlog order' },
        cards: [
          { id: 4591, title: 'Message backlog order', step: 'werk-test' },
          { id: 4595, title: 'Clearing v2 streams', step: 'werk-build' },
        ],
        stream: [
          { action: 'ran tests', timestamp: '2026-10-18T12:35:00Z' },
          { action: 'updated views.ts', timestamp: '2026-10-18T12:30:00Z' },
        ],
        sessions: ['pane · live'],
      },
      {
        id: 'wren',
        name: 'Wren',
        state: 'waiting',
        activity: 'idle',
        card: null,
        cards: [],
        stream: [
          { action: 'waiting on review', timestamp: '2026-10-18T12:00:00Z' },
        ],
        sessions: ['pane · live'],
      },
      {
        id: 'silas',
        name: 'Silas',
        state: 'available',
        activity: 'idle',
        card: null,
        cards: [],
        stream: [],
        sessions: ['pane · live'],
      },
      {
        id: 'kade',
        name: 'Kade',
        state: 'off',
        activity: 'idle',
        card: null,
        cards: [],
        stream: [],
        sessions: [],
      },
    ],
  };

  test('AC1 & AC2: Rooms tab displays the actor rail with all 4 roles and streams from pulse', () => {
    const roomsData: RoomsData = {
      room: 'team',
      channels: ['team', 'gathering'],
      directRoles: ['Wren', 'Silas', 'Kade', 'Abby Normal'],
      actors: mockRolesData.actors,
      messages: [],
    };

    const html = renderRoomsPage('team', roomsData, []);
    expect(html).toContain('id="rooms-actor-rail"');
    expect(html).toContain('actor-tile-wren');
    expect(html).toContain('actor-tile-silas');
    expect(html).toContain('actor-tile-kade');
    expect(html).toContain('actor-tile-abby-normal');

    // Abby shows WIP cards in rail
    expect(html).toContain('#4591, #4595');
    // Stream drawer markup is present
    expect(html).toContain('id="stream-drawer"');
    expect(html).toContain('openRoleStream');
  });

  test('AC5: Rooms tab shows andon light with motion for working roles, amber for waiting, grey for idle', () => {
    const abbyActor = mockRolesData.actors.find((a) => a.id === 'abby-normal');
    const wrenActor = mockRolesData.actors.find((a) => a.id === 'wren');
    const silasActor = mockRolesData.actors.find((a) => a.id === 'silas');

    const abbyAndon = roleAndonLight(abbyActor);
    const wrenAndon = roleAndonLight(wrenActor);
    const silasAndon = roleAndonLight(silasActor);

    expect(abbyAndon.status).toBe('working');
    expect(abbyAndon.cssClass).toBe('andon-working'); // moving/pulsing

    expect(wrenAndon.status).toBe('waiting');
    expect(wrenAndon.cssClass).toBe('andon-waiting');

    expect(silasAndon.status).toBe('idle');
    expect(silasAndon.cssClass).toBe('andon-idle');
  });

  test('AC6: Rooms tab displays ROOMS and DIRECT lists on one line with andon light beside each name', () => {
    const roomsData: RoomsData = {
      room: 'team',
      channels: ['team', 'gathering', 'chorus'],
      directRoles: ['Wren', 'Silas', 'Kade', 'Abby Normal'],
      actors: mockRolesData.actors,
      messages: [],
    };

    const html = renderRoomsPage('team', roomsData, []);
    expect(html).toContain('rooms-nav-inline');
    expect(html).toContain('Rooms');
    expect(html).toContain('Direct');
    // Each direct role has andon dot beside name
    expect(html).toContain('andon-dot andon-waiting'); // Wren
    expect(html).toContain('andon-dot andon-working'); // Abby Normal
  });

  test('AC7 & AC8: Thinking text is marked as thinking (not reply) and toggle controls visibility', () => {
    const roomsData: RoomsData = {
      room: 'team',
      channels: ['team'],
      directRoles: ['Wren'],
      actors: mockRolesData.actors,
      messages: [
        {
          id: 'm1',
          from: 'wren',
          text: 'Thinking through card ordering and backlog replay...',
          time: '12:30',
          tick: true,
          type: 'thinking',
        },
        {
          id: 'm2',
          from: 'wren',
          text: 'Here is the response after turn completion.',
          time: '12:31',
          tick: true,
          type: 'role-response',
        },
      ],
    };

    const html = renderRoomsPage('team', roomsData, []);
    // Thinking message has msg-thinking class and badge-thinking
    expect(html).toContain('msg-thinking');
    expect(html).toContain('badge-thinking');
    expect(html).toContain('thinking-toggle-btn');
    expect(html).toContain('Thinking: ON');
    expect(html).toContain('toggleThinking()');
  });

  test('AC9: Teammates and Board show every WIP card a role holds (0..n cards) with steps', () => {
    const htmlRoles = renderRolesPage(mockRolesData, []);
    // Abby holds 2 cards: both #4591 and #4595 appear
    expect(htmlRoles).toContain('#4591');
    expect(htmlRoles).toContain('#4595');
    // Value stream step per card shows in Activity column (werk-test, werk-build)
    expect(htmlRoles).toContain('werk-test');
    expect(htmlRoles).toContain('werk-build');
    // Silas holds 0 cards: shows dash – and idle activity
    expect(htmlRoles).toContain('–');
    expect(htmlRoles).toContain('idle');

    // Board page renders all WIP cards
    const columns: BoardColumn[] = [
      {
        name: 'WIP',
        ok: true,
        cards: [
          { id: 4591, title: 'Message backlog order', owner: 'Abby-normal' },
          { id: 4595, title: 'Clearing v2 streams', owner: 'Abby-normal' },
        ],
      },
    ];

    const htmlBoard = renderBoardPage(columns, null, [], mockRolesData);
    expect(htmlBoard).toContain('#4591');
    expect(htmlBoard).toContain('#4595');
    // Shows card owner presence from pulse read
    expect(htmlBoard).toContain('presence-working');
  });

  test('Teammates: pairing roles both show the card with pairing with partner (role)', () => {
    const pairRolesData: RolesData = {
      actors: [
        {
          id: 'abby-normal',
          name: 'Abby Normal',
          state: 'pairing',
          activity: 'acting',
          pair: 'pairing with wren (driver)',
          card: { id: 4595, title: 'Clearing v2 streams', step: 'werk-test' },
          cards: [{ id: 4595, title: 'Clearing v2 streams', step: 'werk-test' }],
          sessions: ['pane · live'],
        },
        {
          id: 'wren',
          name: 'Wren',
          state: 'pairing',
          activity: 'acting',
          pair: 'pairing with abby-normal (navigator)',
          card: { id: 4595, title: 'Clearing v2 streams', step: 'werk-test' },
          cards: [{ id: 4595, title: 'Clearing v2 streams', step: 'werk-test' }],
          sessions: ['pane · live'],
        },
      ],
    };

    const html = renderRolesPage(pairRolesData, []);
    expect(html).toContain('pairing with wren (driver)');
    expect(html).toContain('pairing with abby-normal (navigator)');
    expect(html).toContain('werk-test');
  });

  test('Negative proof: a role inactive for 10 minutes shows as quiet, not working', () => {
    const inactiveActor = {
      id: 'kade',
      name: 'Kade',
      state: 'building',
      activity: 'idle', // inactive for 10 minutes
      card: { id: 4602, title: 'Pipeline start other owner' },
      cards: [{ id: 4602, title: 'Pipeline start other owner' }],
      sessions: [],
    };

    const presence = ownerPresence('Kade', { actors: [inactiveActor] });
    expect(presence.status).toBe('quiet');
    expect(presence.text).toBe('quiet');
    expect(presence.status).not.toBe('working');

    const columns: BoardColumn[] = [
      {
        name: 'WIP',
        ok: true,
        cards: [
          { id: 4602, title: 'Pipeline start other owner', owner: 'Kade' },
        ],
      },
    ];

    const htmlBoard = renderBoardPage(columns, null, [], { actors: [inactiveActor] });
    expect(htmlBoard).toContain('presence-quiet');
    expect(htmlBoard).not.toContain('presence-working');
  });

  test('toActorRow passes through 0..n cards and stream from pulse envelope', () => {
    const pulseActor: PulseActor = {
      aboutPrincipal: 'principal-abby-normal',
      name: 'Abby Normal',
      stateValue: 'building',
      activity: 'acting',
      currentCard: 4591,
      cards: [
        { id: 4591, title: 'Message backlog' },
        { id: 4595, title: 'Streams' },
      ],
      stream: [
        { action: 'compile', timestamp: '2026-10-18T12:00:00Z' },
      ],
    };

    const row = toActorRow(pulseActor);
    expect(row.cards).toHaveLength(2);
    expect(row.cards?.[0].id).toBe(4591);
    expect(row.cards?.[1].id).toBe(4595);
    expect(row.stream).toHaveLength(1);
    expect(row.stream?.[0].action).toBe('compile');
  });
});

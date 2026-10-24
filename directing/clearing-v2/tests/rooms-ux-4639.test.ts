// @test-type: unit — Clearing v2 Rooms UX improvements (#4639)
// @card: #4639
// @owner: abby-normal
// @domain: messages

import { renderPeopleRow, renderRoomsPage, isRoleLoggedIn } from '../src/views';
import { toDrawerCard, DrawerCard } from '../src/sources/card';
import { RoomsData, ActorRow } from '../src/sources/models';

describe('#4639 Clearing Rooms UX improvements', () => {
  const activeActors: ActorRow[] = [
    {
      id: 'wren',
      name: 'Wren',
      state: 'building',
      activity: 'acting',
      sessions: ['pane · live · 05:10'],
      cards: [{ id: 4635, title: 'Photos' }],
    },
    {
      id: 'silas',
      name: 'Silas',
      state: 'building',
      activity: 'acting',
      sessions: ['pane · live · 05:08'],
      cards: [{ id: 4707, title: 'Ontology' }],
    },
    {
      id: 'kade',
      name: 'Kade',
      state: 'building',
      activity: 'acting',
      sessions: ['pane · live · 05:05'],
      cards: [],
    },
    {
      id: 'abby-normal',
      name: 'Abby Normal',
      state: 'building',
      activity: 'acting',
      sessions: ['pane · live · 05:10'],
      cards: [{ id: 4639, title: 'Rooms UX' }],
    },
    {
      id: 'borg-queen',
      name: 'Borg-queen',
      state: 'available',
      activity: 'idle',
      sessions: ['+1 not live'],
      cards: [],
    },
  ];

  const mockRoomsData: RoomsData = {
    room: 'team',
    channels: ['team', 'gathering'],
    directRoles: ['Wren', 'Silas', 'Kade', 'Abby Normal'],
    messages: [],
    relayConnected: true,
    actors: activeActors,
  };

  describe('Role Tiles (Compact, No Circles, No Team Tile, Logged-in Roles Only)', () => {
    test('renderPeopleRow renders only logged-in roles without #team tile', () => {
      const html = renderPeopleRow('team', activeActors);

      // No #team tile
      expect(html).not.toContain('team-tile');
      expect(html).not.toContain('#team');

      // Logged in roles are rendered
      expect(html).toContain('data-role="wren"');
      expect(html).toContain('data-role="silas"');
      expect(html).toContain('data-role="kade"');
      expect(html).toContain('data-role="abby-normal"');

      // Shows names and cards
      expect(html).toContain('<b>Wren</b>');
      expect(html).toContain('#4635');
      expect(html).toContain('<b>Silas</b>');
      expect(html).toContain('#4707');
      expect(html).toContain('<b>Abby Normal</b>');
      expect(html).toContain('#4639');
      expect(html).toContain('no card'); // Kade has no card
    });

    test('renderPeopleRow drops circular avatars and retains andon dot flasher (#4723)', () => {
      const html = renderPeopleRow('team', activeActors);

      // No circle avatar (<span class="av ...">)
      expect(html).not.toMatch(/<span class="av/);
      // Active andon dot flasher present (#4723)
      expect(html).toContain('andon-dot');
    });

    test('NEGATIVE PROOF: non-logged-in actor (all sessions not live) is excluded from role tiles', () => {
      const html = renderPeopleRow('team', activeActors);
      // Borg Queen has only non-live sessions (+1 not live) -> must NOT appear in role tiles
      expect(html).not.toContain('data-role="borg-queen"');
      expect(html).not.toContain('Borg-queen');

      // Offline actor is also excluded
      const offlineActor: ActorRow = {
        id: 'guest',
        name: 'Guest',
        state: 'offline',
        sessions: [],
        cards: [],
      };
      expect(isRoleLoggedIn(offlineActor)).toBe(false);
      expect(isRoleLoggedIn(activeActors[0])).toBe(true);
      expect(isRoleLoggedIn(activeActors[4])).toBe(false); // Borg Queen
    });

    test('Card references in role tiles have click handler to open card drawer', () => {
      const html = renderPeopleRow('team', activeActors);
      expect(html).toContain("openCardDrawer('4639')");
      expect(html).toContain("openCardDrawer('4635')");
      expect(html).toContain("openCardDrawer('4707')");
    });
  });

  describe('Rooms Composer (Textarea with Enter for CR and Photo Picker)', () => {
    test('Rooms composer uses textarea for composer-input allowing multiline input', () => {
      const html = renderRoomsPage('team', mockRoomsData, []);
      // Positive proof: textarea with id="composer-input"
      expect(html).toContain('<textarea id="composer-input"');
      expect(html).not.toContain('<input type="text" id="composer-input"');
    });

    test('Photo picker has accessible label and file input without display:none hidden attribute', () => {
      const html = renderRoomsPage('team', mockRoomsData, []);
      // Positive proof: label with for="composer-photo" and file input
      expect(html).toContain('for="composer-photo"');
      expect(html).toContain('class="composer-photo"');
      expect(html).toContain('id="composer-photo"');
      expect(html).toContain('accept="image/*"');
      // Must not use HTML hidden attribute which blocks iOS Safari label clicks
      expect(html).not.toMatch(/<input[^>]*id="composer-photo"[^>]*\bhidden\b/);
      // Uses sr-only class for accessibility
      expect(html).toMatch(/<input[^>]*id="composer-photo"[^>]*class="[^"]*sr-only[^"]*"/);
    });
  });

  describe('Card Attributes & Acceptance Criteria in Drawer', () => {
    test('toDrawerCard parses AC items from ac_items array or description markdown', () => {
      const cardWithAc = {
        kind: 'Card',
        data: {
          title: 'Rooms UX Card',
          status: 'WIP',
          owner: 'Abby-normal',
          description: '## AC\n- [ ] Enter for newline\n- [x] Fix photo picker',
          ac_items: [
            { text: 'Enter for newline', checked: false },
            { text: 'Fix photo picker', checked: true },
          ],
        },
      };

      const drawer = toDrawerCard(4639, cardWithAc);
      expect(drawer).not.toBeNull();
      expect(drawer?.title).toBe('Rooms UX Card');
      expect(drawer?.owner).toBe('Abby Normal');
      expect(drawer?.acItems).toEqual([
        { text: 'Enter for newline', checked: false },
        { text: 'Fix photo picker', checked: true },
      ]);
    });

    test('NEGATIVE PROOF: card without AC returns undefined acItems, not an empty broken list', () => {
      const cardWithoutAc = {
        kind: 'Card',
        data: {
          title: 'No AC Card',
          status: 'Next',
          owner: 'kade',
          description: 'Just a simple task without AC',
        },
      };

      const drawer = toDrawerCard(4640, cardWithoutAc);
      expect(drawer?.acItems).toBeUndefined();
    });

    test('Borg Queen messages render with proper name capitalization and role style class', () => {
      const { renderRoomMessage } = require('../src/views');
      const html = renderRoomMessage({
        id: 'msg-bq-1',
        from: 'borg-queen',
        text: 'Prover report: all 13 checks green.',
        time: '06:05',
      });
      expect(html).toContain('who-borg-queen');
      expect(html).toContain('>Borg Queen</span>');
      expect(html).toContain('a-borg-queen');
      // NEGATIVE PROOF: must not render raw uncapitalized slug without role style
      expect(html).not.toContain('>borg-queen</span>');
    });
  });
});

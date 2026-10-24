// @test-type: unit — Clearing v2 Role Flasher & Full Card Attributes in Drawer (#4723)
// @card: #4723
// @owner: abby-normal
// @domain: messages

import { renderPeopleRow } from '../src/views';
import { toDrawerCard, DrawerCard } from '../src/sources/card';
import { ActorRow } from '../src/sources/models';

describe('#4723 Role Flasher & Full Card Attributes in Drawer', () => {
  const activeActors: ActorRow[] = [
    {
      id: 'wren',
      name: 'Wren',
      state: 'building',
      activity: 'acting',
      sessions: ['pane · live · 07:10'],
      cards: [{ id: 4635, title: 'Photos' }],
    },
    {
      id: 'silas',
      name: 'Silas',
      state: 'waiting',
      activity: 'idle',
      sessions: ['pane · live · 07:08'],
      cards: [{ id: 4717, title: 'Ontology' }],
    },
    {
      id: 'kade',
      name: 'Kade',
      state: 'available',
      activity: 'idle',
      sessions: ['pane · live · 07:05'],
      cards: [],
    },
    {
      id: 'abby-normal',
      name: 'Abby Normal',
      state: 'building',
      activity: 'acting',
      sessions: ['pane · live · 07:10'],
      cards: [{ id: 4723, title: 'Role Flasher & Drawer' }],
    },
  ];

  describe('Active Role Flasher on Role Tiles', () => {
    test('role tiles display andon-dot flasher with correct status classes', () => {
      const html = renderPeopleRow('team', activeActors);
      // Working/acting actors get andon-working flasher
      expect(html).toContain('andon-dot andon-working');
      // Waiting actors get andon-waiting
      expect(html).toContain('andon-dot andon-waiting');
    });

    test('NEGATIVE PROOF: idle actor does not get andon-working pulse flasher', () => {
      const html = renderPeopleRow('team', activeActors);
      const kadeTileMatch = html.match(/data-role="kade"[\s\S]*?<\/a>/);
      expect(kadeTileMatch).not.toBeNull();
      const kadeTile = kadeTileMatch![0];
      expect(kadeTile).not.toContain('andon-working');
      expect(kadeTile).toContain('andon-idle');
    });
  });

  describe('Full Card Attributes in Drawer', () => {
    test('toDrawerCard parses all attributes: domains, priority, status, created, updated, description, AC, comments', () => {
      const payload = {
        kind: 'Card',
        data: {
          label: 'Test Card Title',
          status: 'WIP',
          priority: 'P1',
          creator: 'jeff',
          owner: 'Silas',
          created: '2026-10-23T21:24:18Z',
          modified: '2026-10-24T10:06:44Z',
          domains: ['domain:chorus', 'type:fix', 'sequence:framework'],
          description: '## Experience\nTesting experience details.\n\n## AC\n- [ ] AC Item 1\n- [x] AC Item 2',
        },
        comments: [
          { author: 'jeff', text: 'Important comment from Jeff' },
        ],
      };

      const card: DrawerCard | null = toDrawerCard(4717, payload);
      expect(card).not.toBeNull();
      expect(card!.id).toBe(4717);
      expect(card!.title).toBe('Test Card Title');
      expect(card!.status).toBe('WIP');
      expect(card!.owner).toBe('Silas');
      expect(card!.priority).toBe('P1');
      expect(card!.domain).toContain('domain:chorus');
      expect(card!.created).toBe('2026-10-23T21:24:18Z');
      expect(card!.updated).toBe('2026-10-24T10:06:44Z');
      expect(card!.acItems?.length).toBe(2);
      expect(card!.comments?.length).toBe(1);
    });

    test('NEGATIVE PROOF: missing optional fields do not fail card parsing', () => {
      const minimalPayload = {
        kind: 'Card',
        data: {
          label: 'Minimal Card',
        },
      };

      const card = toDrawerCard(1234, minimalPayload);
      expect(card).not.toBeNull();
      expect(card!.id).toBe(1234);
      expect(card!.title).toBe('Minimal Card');
      expect(card!.domain).toBeUndefined();
      expect(card!.created).toBeUndefined();
      expect(card!.updated).toBeUndefined();
    });
  });
});

// @test-type: unit
// @domain: cards
// @card: #4574
// @owner: abby-normal
// #4574 rework: phone-width horizontal scroll for board columns, card cards with
// owner metadata, and tap-to-open drawer.

import fs from 'fs';
import path from 'path';
import { renderBoardPage } from '../src/views';
import type { BoardColumn } from '../src/sources/board';

describe('#4574 Board Rework', () => {
  test('renderBoardPage renders board-cols, columns, and tap-to-drawer card cards', () => {
    const cols: BoardColumn[] = [
      {
        name: 'WIP',
        ok: true,
        cards: [
          { id: 4574, title: 'Clearing v2 Board and Alerts', owner: 'Abby-normal' },
          { id: 4585, title: 'Fix sensor LaunchAgents', owner: 'Silas' },
        ],
      },
      {
        name: 'Next',
        ok: true,
        cards: [{ id: 4595, title: 'Streams and lights', owner: 'Wren' }],
      },
      {
        name: 'Done',
        ok: false,
        reason: 'no board API endpoint for done cards',
      },
    ];

    const html = renderBoardPage(cols, null, []);

    // Structure checks
    expect(html).toContain('class="board-cols"');
    expect(html).toContain('class="board-col"');
    expect(html).toContain('#4574');
    expect(html).toContain('Clearing v2 Board and Alerts');
    expect(html).toContain('Abby-normal');
    expect(html).toContain("openCardDrawer('4574')");
    expect(html).toContain('data-open-card="4574"');
    expect(html).toContain('card-drawer');
    expect(html).toContain('cws-motion-bar');

    // Negative proof: unmeasured column renders reason, not 0 cards
    expect(html).toContain('board-unmeasured');
    expect(html).toContain('no board API endpoint for done cards');
  });

  test('renderBoardPage renders presence indicator and metadata for cards with owner', () => {
    const cols: BoardColumn[] = [
      {
        name: 'WIP',
        ok: true,
        cards: [
          { id: 4574, title: 'Clearing v2 Board and Alerts', owner: 'Abby-normal' },
        ],
      },
    ];

    const html = renderBoardPage(cols, null, []);
    expect(html).toContain('board-card-title');
    expect(html).toContain('board-card-meta');
    expect(html).toContain('Abby-normal');
  });

  test('negative proof: empty board column renders 0 cards without crashing', () => {
    const cols: BoardColumn[] = [
      { name: 'WIP', ok: true, cards: [] },
    ];
    const html = renderBoardPage(cols, null, []);
    expect(html).toContain('0 cards');
  });

  test('negative proof: board column without ok status shows unmeasured reason', () => {
    const cols: BoardColumn[] = [
      { name: 'Next', ok: false, reason: 'source unavailable' },
    ];
    const html = renderBoardPage(cols, null, []);
    expect(html).toContain('board-unmeasured');
    expect(html).toContain('source unavailable');
  });

  test('board page renders columns that match phone 85vw scroll-snap styling', () => {
    const html = renderBoardPage([{ name: 'WIP', ok: true, cards: [] }], null, []);
    expect(html).toContain('board-cols');
    const css = fs.readFileSync(path.join(__dirname, '..', 'public', 'style.css'), 'utf8');
    const phoneIdx = css.indexOf('@media (max-width: 480px)');
    expect(phoneIdx).toBeGreaterThan(-1);
    const phoneBlock = css.slice(phoneIdx);
    expect(phoneBlock).toMatch(/\.board-cols\s*\{[^}]*scroll-snap-type:\s*x mandatory;/);
    expect(phoneBlock).toMatch(/\.board-col\s*\{[^}]*flex:\s*0 0 85vw;/);
  });
});

// @test-type: unit — tests diff-in-place for refreshPeopleRow without replacing row DOM node or shifting scroll
// @card: #4749
// @domain: messages

import { ClearingMessage } from '../src/types';

const app = require('../public/app.js');

function createMockElement(id: string, tag = 'div') {
  const classListSet = new Set<string>();
  const el: any = {
    id,
    tagName: tag.toUpperCase(),
    className: '',
    classList: {
      add: (c: string) => classListSet.add(c),
      remove: (c: string) => classListSet.delete(c),
      contains: (c: string) => classListSet.has(c),
    },
    style: { display: '', height: '' },
    dataset: {} as Record<string, string>,
    children: [] as any[],
    textContent: '',
    innerHTML: '',
    outerHTML: '',
    addEventListener: jest.fn(),
    appendChild(c: any) { this.children.push(c); },
    querySelector(sel: string) {
      if (sel.startsWith('.')) {
        const cls = sel.slice(1);
        return this.children.find((c: any) => c.classList && c.classList.contains(cls)) || null;
      }
      return null;
    },
    querySelectorAll(sel: string) { return this.children; },
  };
  return el;
}

describe('#4749 AC3: layout stability via diff-in-place for people row and tiles', () => {
  test('refreshPeopleRow updates tile contents in-place, keeping the exact same DOM node reference and scroll position', async () => {
    const stream = createMockElement('messages-stream');
    stream.scrollTop = 250;
    stream.scrollHeight = 1000;

    const peopleRow = createMockElement('rooms-people-row');
    const tileWren = createMockElement('actor-tile-wren', 'a');
    tileWren.className = 'actor-tile actor-tile-wren';
    tileWren.dataset = { role: 'wren' };

    const andonDot = createMockElement('andon-dot-wren', 'span');
    andonDot.className = 'andon-dot andon-idle';
    const cardEl = createMockElement('card-wren', 'div');
    cardEl.className = 'actor-tile-card';
    cardEl.textContent = '#4743';

    tileWren.children.push(andonDot, cardEl);
    tileWren.querySelector = (sel: string) => {
      if (sel.includes('andon-dot')) return andonDot;
      if (sel.includes('actor-tile-card')) return cardEl;
      return null;
    };

    peopleRow.children.push(tileWren);
    peopleRow.querySelector = (sel: string) => {
      if (sel.includes('actor-tile-wren') || sel.includes('[data-role="wren"]')) return tileWren;
      return null;
    };
    peopleRow.querySelectorAll = (sel: string) => [tileWren];

    (global as any).document = {
      getElementById: (id: string) => {
        if (id === 'rooms-people-row') return peopleRow;
        if (id === 'messages-stream') return stream;
        if (id === 'room-picker') return { value: 'team' };
        return null;
      },
      createElement: (tag: string) => createMockElement(`temp-${Math.random()}`, tag),
    };

    // Keep reference to original tile DOM element
    const originalTileNode = tileWren;

    // Simulate backend sending updated HTML where wren moved from card #4743 to #4749 and andon becomes building
    const updatedHtml = `
      <div class="actor-rail people-row" id="rooms-people-row">
        <a class="actor-tile actor-tile-wren" data-role="wren" href="/rooms/wren">
          <div class="actor-tile-info">
            <div class="actor-tile-name"><span class="andon-dot andon-building"></span><b>Wren</b></div>
            <div class="actor-tile-card"><span class="cid card-ref">#4749</span></div>
          </div>
        </a>
      </div>
    `;

    const mockFetch = async () => ({
      ok: true,
      json: async () => ({ html: updatedHtml })
    });

    await app.refreshPeopleRow(mockFetch);

    // AC3 INVARIANT: The node reference must NOT have been replaced by an outerHTML blowup!
    const currentPeopleRow = (global as any).document.getElementById('rooms-people-row');
    expect(currentPeopleRow).toBe(peopleRow);

    const wrenNodeAfter = currentPeopleRow.querySelector('.actor-tile-wren');
    expect(wrenNodeAfter).toBe(originalTileNode);

    // State must be updated in-place
    expect(andonDot.className).toContain('andon-building');
    expect(cardEl.innerHTML).toContain('#4749');

    // Scroll must not shift
    expect(stream.scrollTop).toBe(250);
  });
});

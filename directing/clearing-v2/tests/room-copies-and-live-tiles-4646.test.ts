// @test-type: unit — page script with a stand-in document, and the server's own renderers; no browser.
// @card: #4646
// @owner: wren
// @domain: messages
/**
 * #4646 — Jeff 10-19 12:51 (phone screenshot): every reply showed twice; 12:53: "look at
 * your role tile - it shows 4635 wip and ur message says its done". The room merged the
 * second copy but still pushed it live; the tiles were drawn once and never refreshed.
 */
import { renderPeopleRow, renderRoomMessage, bodyHash } from '../src/views';
const app = require('../public/app.js');

function room(rendered: Array<{ from: string; body: string; ts: string }>) {
  const children: any[] = rendered.map((r) => ({ id: 'x', dataset: { from: r.from, body: bodyHash(r.body), ts: r.ts } }));
  const container: any = { children, appendChild: (c: any) => children.push(c), scrollTop: 0, scrollHeight: 0 };
  const el = () => ({ dataset: {} as any, children: [] as any[], appendChild(c: any) { this.children.push(c); }, set className(_v: string) {}, textContent: '' });
  (global as any).document = { getElementById: (id: string) => (id === 'messages-stream' ? container : null), createElement: el, createTextNode: (t: string) => ({ textContent: t }) };
  return container;
}

describe('#4646 a second copy of a message is not shown twice', () => {
  afterEach(() => { delete (global as any).document; });

  test('the same author and words arriving seconds later, under a new id, is dropped', () => {
    const c = room([{ from: 'wren', body: 'Yes, #4635 is done.', ts: '2026-10-19T16:50:10.000Z' }]);
    app.appendMessage({ id: 'copy-2', from: 'wren', text: 'Yes, #4635 is done.', ts: '2026-10-19T16:50:12.000Z', visible: true });
    expect(c.children).toHaveLength(1);
  });

  test('NEGATIVE PROOF: the same words a minute later are a repeat and do show', () => {
    const c = room([{ from: 'jeff', body: 'go', ts: '2026-10-19T16:50:00.000Z' }]);
    app.appendMessage({ id: 'again', from: 'jeff', text: 'go', ts: '2026-10-19T16:51:30.000Z', visible: true });
    expect(c.children).toHaveLength(2);
  });

  test('a message drawn by the server carries the author, fingerprint and time the page compares', () => {
    const html = renderRoomMessage({ id: 'm1', from: 'wren', text: 'hello **there**', time: '12:50', ts: '2026-10-19T16:50:10.000Z', tick: true } as any);
    expect(html).toContain('data-from="wren"');
    expect(html).toContain(`data-body="${bodyHash('hello **there**')}"`);
    expect(html).toContain('data-ts="2026-10-19T16:50:10.000Z"');
  });
});

describe('#4646 the role tiles stay true while the page is open', () => {
  afterEach(() => { delete (global as any).document; });

  test('a tile shows the cards pulse reports now: a closed card disappears', () => {
    const wip = renderPeopleRow('team', [{ id: 'wren', name: 'Wren', state: 'building', sessions: [], cards: [{ id: 4635, title: 'photos' }], stream: [] } as any]);
    const done = renderPeopleRow('team', [{ id: 'wren', name: 'Wren', state: 'available', sessions: [], cards: [], stream: [] } as any]);
    expect(wip).toContain('#4635');
    expect(done).not.toContain('#4635');
    expect(done).toContain('no card');
  });

  test('the page swaps in the fresh tiles it fetches', async () => {
    const row: any = { outerHTML: '<div id="rooms-people-row">#4635</div>' };
    (global as any).document = { getElementById: (id: string) => (id === 'rooms-people-row' ? row : id === 'room-picker' ? { value: 'team' } : null) };
    let asked = '';
    const ok = await app.refreshPeopleRow(async (u: string) => { asked = u; return { ok: true, json: async () => ({ html: '<div id="rooms-people-row">no card</div>' }) }; });
    expect(ok).toBe(true);
    expect(asked).toBe('/api/rooms/team/people');
    expect(row.outerHTML).toContain('no card');
  });
});

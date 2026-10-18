// @test-type: unit — renders the room and reads notes in-process; no live relay, no DOM.
// @card: #4592
// @owner: silas
// @domain: messages
/**
 * #4592 — nudges between roles and system alerts now reach the room (v1 tags
 * each note kind=nudge or kind=alert). Jeff, 10-17 20:50: a "team nudges"
 * toggle in the v2 room hides or shows them, on by default to match the old
 * Clearing, remembered per device.
 */
import { kindOf } from '../src/server';
import { renderRoomsPage } from '../src/views';
const { teamNudgesOn, setTeamNudges } = require('../public/app.js');

type Msg = { id: string; from: string; text: string; kind?: 'nudge' | 'alert' };
const data = (messages: Msg[]) => ({
  room: 'team', channels: ['team'], directRoles: [] as string[],
  messages: messages.map((m) => ({ ...m, time: '10:00', tick: false })),
});

/** A Map-backed stand-in for localStorage. */
const storage = () => {
  const m = new Map<string, string>();
  return { keys: () => Array.from(m.keys()), getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => { m.set(k, v); } };
};

describe('#4592 a note says its kind', () => {
  test('kind=nudge and kind=alert tags are read; a note with no kind tag has none', () => {
    expect(kindOf({ tags: [['t', 'team'], ['author', 'kade'], ['kind', 'nudge']] })).toBe('nudge');
    expect(kindOf({ tags: [['t', 'team'], ['author', 'system'], ['kind', 'alert']] })).toBe('alert');
    expect(kindOf({ tags: [['t', 'team'], ['author', 'wren']] })).toBeUndefined();
  });

  // NEGATIVE PROOF: only the two kinds the toggle owns are read; anything else
  // a note might carry never becomes a hideable kind.
  test('negative proof: an unknown kind value is not a kind', () => {
    expect(kindOf({ tags: [['kind', 'reply']] })).toBeUndefined();
  });
});

describe('#4592 the team nudges toggle', () => {
  test('the room marks nudges and alerts by kind and offers the toggle, off by default', () => {
    const html = renderRoomsPage('team', data([
      { id: 'a', from: 'kade', text: '[nudge from kade → silas] ping', kind: 'nudge' },
      { id: 'b', from: 'system', text: '[alert → wren] stale log', kind: 'alert' },
      { id: 'c', from: 'wren', text: 'a reply to Jeff' },
    ]), []);
    expect(html).toMatch(/id="msg-a"[^>]*data-kind="nudge"/);
    expect(html).toMatch(/id="msg-b"[^>]*data-kind="alert"/);
    expect(html).toMatch(/<input[^>]*id="team-nudges"/);
    expect(html).not.toMatch(/<input[^>]*id="team-nudges"[^>]*checked/);
    expect(html).toContain('body.hide-team-nudges [data-kind]');
  });

  // NEGATIVE PROOF: a reply carries no data-kind, so hiding team nudges can
  // never hide an answer to Jeff.
  test('negative proof: a reply is never marked hideable', () => {
    const html = renderRoomsPage('team', data([{ id: 'c', from: 'wren', text: 'a reply to Jeff' }]), []);
    expect(html).not.toMatch(/id="msg-c"[^>]*data-kind/);
  });

  test('off by default, remembered per device, under the key the room page reads', () => {
    const s = storage();
    expect(teamNudgesOn(s)).toBe(false);
    setTeamNudges(s, true);
    expect(teamNudgesOn(s)).toBe(true);
    setTeamNudges(s, false);
    expect(teamNudgesOn(s)).toBe(false);
    // the page's own toggle script must read the same key the helper writes
    const html = renderRoomsPage('team', data([]), []);
    expect(s.keys()).toHaveLength(1);
    expect(html).toContain(s.keys()[0]);
  });
});

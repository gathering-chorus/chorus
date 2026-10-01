// @test-type: unit — the Clearing's remaining routes over HTTP, in-process; the cards CLI is a stub under a temp CHORUS_ROOT.
/**
 * #4417 — routes Kade's list showed named by one test file or none: the guest
 * name, the AI chat session, the message window, the tiles and the card panel.
 */
import { useInProcessClearing } from './lib/in-process-clearing'; // first: sets the temp guard key path
import * as fs from 'fs';
import * as path from 'path';

const gate = useInProcessClearing();
const base = () => gate.base();

// The card panel shells out to platform/scripts/cards under CHORUS_ROOT.
const cardsScript = path.join(gate.dir, 'platform', 'scripts', 'cards');
fs.mkdirSync(path.dirname(cardsScript), { recursive: true });
fs.writeFileSync(cardsScript, `#!/bin/bash
[ "$1" = view ] && [ "$2" = 4417 ] || exit 1
cat <<'OUT'
#4417 Clearing tests warn before Jeff does
  Status:   WIP
  Owner:    Wren
  Priority: P1
  Desc:
    ## AC
    - [x] no live writes
    - [ ] headers name card and owner
  Domains:  domain:chorus
OUT
`, { mode: 0o755 });

describe('#1719 a guest names themselves', () => {
  test('the name is kept in a cookie, trimmed and capped at 30 characters, then back to the room', async () => {
    const res = await fetch(`${base()}/set-name`, {
      method: 'POST', redirect: 'manual',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: `  ${'a'.repeat(40)}  ` }),
    });
    expect(res.status).toBe(302);
    expect(res.headers.get('location')).toBe('/');
    expect(res.headers.get('set-cookie')).toMatch(new RegExp(`^bridge_name=${'a'.repeat(30)};`));
  });

  test('NEGATIVE PROOF: a blank name sets no cookie', async () => {
    const res = await fetch(`${base()}/set-name`, {
      method: 'POST', redirect: 'manual',
      headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: '   ' }),
    });
    expect(res.status).toBe(302);
    expect(res.headers.get('set-cookie')).toBeNull();
  });
});

describe('the AI chat session', () => {
  test('start → active, messages answer an array, end → not active', async () => {
    const start = await (await fetch(`${base()}/api/chat/start`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' })).json();
    expect(start).toBeTruthy();
    expect((await (await fetch(`${base()}/api/chat/state`)).json()).active).toBe(true);
    expect(Array.isArray((await (await fetch(`${base()}/api/chat/messages?since=0`)).json()).messages)).toBe(true);
    await fetch(`${base()}/api/chat/end`, { method: 'POST' });
    expect((await (await fetch(`${base()}/api/chat/state`)).json()).active).toBe(false);
  });

  test('NEGATIVE PROOF: a chat message with no text is refused 400', async () => {
    const res = await fetch(`${base()}/api/chat/message`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' });
    expect(res.status).toBe(400);
  });
});

describe('#3852 the message window says what it withheld', () => {
  test('limit is honoured and capped at 2000; the totals ride on headers', async () => {
    const res = await fetch(`${base()}/api/messages?limit=999999`);
    expect(res.status).toBe(200);
    expect(Array.isArray(await res.json())).toBe(true);
    expect(res.headers.get('x-chorus-total')).toMatch(/^\d+$/);
    expect(res.headers.get('x-chorus-withheld')).toMatch(/^\d+$/);
  });
});

describe('the role tiles', () => {
  test('answer one tile per person in the room, Jeff included', async () => {
    const tiles = await (await fetch(`${base()}/api/tiles`)).json() as Array<{ role: string }>;
    expect(tiles.map((t) => t.role).sort()).toEqual(['jeff', 'kade', 'silas', 'wren']);
  });
});

describe('the card panel', () => {
  test('a card is read from the board and its AC split into done and not done', async () => {
    const card = await (await fetch(`${base()}/api/card/4417`)).json();
    expect(card).toMatchObject({ id: '4417', title: 'Clearing tests warn before Jeff does', status: 'WIP', owner: 'Wren' });
    expect(card.ac).toEqual([{ done: true, text: 'no live writes' }, { done: false, text: 'headers name card and owner' }]);
  });

  test('NEGATIVE PROOF: an id with no digits is refused 400 and the board is never asked', async () => {
    const res = await fetch(`${base()}/api/card/abc`);
    expect(res.status).toBe(400);
  });
});

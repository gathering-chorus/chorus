// @test-type: unit — Clearing v2 presents what the three pulse APIs return (actor-state, changes-overnight, readings), against a local stub, no live services (#4563)
// @card: #4563
// @owner: wren
// @domain: messages
//
// Jeff 2026-10-16 08:13: "we use the data from our apis and donnt manufacture or do data
// processing in our ui". The page shows the word pulse sends; it never derives a state,
// diffs cards, or probes sources itself. When pulse cannot answer, the page says
// "unmeasured" with pulse's (or the fetch's) reason, never a zero or a silent empty list.

import fs from 'fs';
import path from 'path';
import http from 'http';
import { createApp } from '../src/server';

type Json = Record<string, unknown>;

let stub: http.Server;
let stubBase = '';
const fixtures: Record<string, { status: number; body: Json }> = {};

function actorStateFixture(): Json {
  return {
    derivedAt: '2026-10-16T14:00:00Z',
    actors: [
      {
        aboutPrincipal: 'principal-zz-pilot',
        name: 'ZZ Pilot',
        stateValue: 'pairing',
        stateSource: 'observed',
        activity: 'acting',
        pairRole: 'driver',
        pairedWith: 'principal-zz-navigator',
        currentCard: 9911,
        currentCardTitle: 'Fly the stub plane',
        sessions: [{ id: 's1', channel: 'terminal', live: true, lastActionAt: '2026-10-16T13:58:00Z' }],
      },
      {
        aboutPrincipal: 'principal-zz-navigator',
        stateValue: 'available',
        stateSource: 'observed',
        activity: 'idle',
        currentCard: null,
        sessions: [],
      },
      {
        aboutPrincipal: 'principal-zz-waiter',
        name: 'ZZ Waiter',
        stateValue: 'waiting',
        stateSource: 'observed',
        activity: 'acting',
        currentCard: 9922,
        waitingOn: { on: 'principal-zz-pilot', what: 'reply', since: '2026-10-16 09:00:00' },
        sessions: [],
      },
    ],
    unmeasured: [],
  };
}

function changesFixture(): Json {
  return {
    derivedAt: '2026-10-16T14:00:00Z',
    watermark: '2026-10-15T22:00:00Z',
    cards: [
      { id: 9931, title: 'Stub card that landed', status: 'Done', owner: 'principal-zz-pilot', movedAt: '2026-10-16T02:00:00Z' },
      { id: 9932, title: 'Stub card now blocked', status: 'Blocked', owner: 'principal-zz-waiter', movedAt: '2026-10-16T03:00:00Z' },
    ],
    unmeasured: [],
  };
}

function readingsFixture(): Json {
  return {
    derivedAt: '2026-10-16T14:00:00Z',
    readings: [
      { source: 'zz-board', observedAt: '2026-10-16T14:00:00Z', status: 'ok', lastWrite: '2026-10-16T13:59:00Z', asOf: 'as of 09:59' },
      { source: 'zz-spine', observedAt: '2026-10-16T14:00:00Z', status: 'ok', lastWrite: '2026-10-16T13:20:00Z', asOf: 'as of 09:20', lagMinutes: 40 },
      { source: 'zz-sessions', observedAt: '2026-10-16T14:00:00Z', status: 'unmeasured', reason: 'sessions door timed out' },
    ],
    unmeasured: [],
  };
}

function resetFixtures(): void {
  fixtures['/api/actor-state'] = { status: 200, body: actorStateFixture() };
  fixtures['/api/changes-overnight'] = { status: 200, body: changesFixture() };
  fixtures['/api/readings'] = { status: 200, body: readingsFixture() };
  fixtures['/alerts'] = { status: 200, body: { data: [] } };
}

beforeAll(async () => {
  stub = http.createServer((req, res) => {
    const f = fixtures[(req.url || '').split('?')[0]];
    if (!f) { res.writeHead(404); res.end(); return; }
    res.writeHead(f.status, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(f.body));
  });
  await new Promise<void>((r) => stub.listen(0, '127.0.0.1', () => r()));
  stubBase = `http://127.0.0.1:${(stub.address() as { port: number }).port}`;
});
afterAll(async () => { await new Promise<void>((r) => stub.close(() => r())); });
beforeEach(resetFixtures);

async function page(route: string, pulseUrl = stubBase): Promise<string> {
  const { app } = createApp({
    relayUrl: 'ws://127.0.0.1:3499',
    pulseUrl,
    rolesUrl: `${stubBase}/roles`,
    alertsUrl: `${stubBase}/alerts`,
    cardsUrl: `${stubBase}/cards`,
  });
  const s = http.createServer(app);
  await new Promise<void>((r) => s.listen(0, '127.0.0.1', () => r()));
  try {
    const port = (s.address() as { port: number }).port;
    return await (await fetch(`http://127.0.0.1:${port}${route}`)).text();
  } finally {
    await new Promise<void>((r) => s.close(() => r()));
  }
}

function section(html: string, startMarker: string, endMarker?: string): string {
  const start = html.indexOf(startMarker);
  expect(start).toBeGreaterThanOrEqual(0);
  const end = endMarker ? html.indexOf(endMarker, start) : -1;
  return end > start ? html.slice(start, end) : html.slice(start);
}

describe('a) roles and Working now show actor-state words verbatim', () => {
  test('Roles page shows each stateValue, pair partner and card from the fixture', async () => {
    const html = await page('/roles');
    expect(html).toContain('ZZ Pilot');
    expect(html).toContain('state-word">pairing</span>');
    expect(html).toContain('state-word">available</span>');
    expect(html).toContain('state-word">waiting</span>');
    expect(html).toContain('zz-navigator'); // no name sent: the id without "principal-"
    expect(html).not.toContain('principal-zz-navigator</');
    expect(html).toContain('driver with zz-navigator');
    expect(html).toContain('#9911');
    expect(html).toContain('Fly the stub plane');
    expect(html).toContain('#9922'); // no title sent: the card as "#<id>"
    expect(html).toContain('on zz-pilot');
  });

  test('Today Working now shows each stateValue, pair partner and card from the fixture', async () => {
    const working = section(await page('/today'), 'working-now');
    expect(working).toContain('ZZ Pilot');
    expect(working).toContain('state-word">pairing</span>');
    expect(working).toContain('state-word">available</span>');
    expect(working).toContain('driver with zz-navigator');
    expect(working).toContain('#9911');
    expect(working).toContain('Fly the stub plane');
  });

  test('changing the fixture stateValue changes the page, no code change', async () => {
    const body = actorStateFixture();
    (body.actors as Json[])[0].stateValue = 'in-ceremony';
    fixtures['/api/actor-state'] = { status: 200, body };
    const roles = await page('/roles');
    const working = section(await page('/today'), 'working-now');
    for (const html of [roles, working]) {
      expect(html).toContain('state-word">in-ceremony</span>');
      expect(html).not.toContain('state-word">pairing</span>');
    }
  });
});

describe('b) Changed overnight lists exactly the fixture cards', () => {
  test('every fixture card with its title and status, and nothing else', async () => {
    const changed = section(await page('/today'), 'changed-overnight', 'alerts-section');
    expect(changed).toContain('#9931');
    expect(changed).toContain('Stub card that landed');
    expect(changed).toContain('>Done</span>');
    expect(changed).toContain('#9932');
    expect(changed).toContain('Stub card now blocked');
    expect(changed).toContain('>Blocked</span>');
    expect((changed.match(/class="cid"/g) || []).length).toBe(2);
  });

  test('a card added to the fixture appears; v2 never diffs or drops it', async () => {
    const body = changesFixture();
    (body.cards as Json[]).push({ id: 9933, title: 'Old card pulse still sent', status: 'Next', owner: 'principal-zz-pilot', movedAt: '2020-01-01T00:00:00Z' });
    fixtures['/api/changes-overnight'] = { status: 200, body };
    const changed = section(await page('/today'), 'changed-overnight', 'alerts-section');
    expect(changed).toContain('#9933');
    expect(changed).toContain('Old card pulse still sent');
  });
});

describe('c) trust strip shows one entry per reading', () => {
  test('each reading once, with its asOf, and an unmeasured reading with its reason', async () => {
    const strip = section(await page('/roles'), 'trust-strip', '</div>');
    for (const src of ['zz-board', 'zz-spine', 'zz-sessions']) {
      expect((strip.match(new RegExp(`${src}<|${src} `, 'g')) || []).length).toBe(1);
    }
    expect(strip).toContain('zz-board · as of 09:59');
    expect(strip).toContain('zz-spine · as of 09:20');
    expect(strip).toContain('fail-source-name">zz-sessions</span> unmeasured (sessions door timed out)');
  });
});

describe('d) NEGATIVE PROOF: pulse unreachable or unmeasured reads unmeasured with the reason, never a zero', () => {
  const DEAD_PULSE = 'http://127.0.0.1:1';

  test('pulse unreachable: Working now, Changed overnight, Roles and strip all say unmeasured', async () => {
    const today = await page('/today', DEAD_PULSE);
    const working = section(today, 'working-now');
    expect(working).toContain('unmeasured');
    expect(working).toContain('actor-state');
    expect(working).not.toContain('0 roles working now');
    const changed = section(today, 'changed-overnight', 'alerts-section');
    expect(changed).toContain('unmeasured');
    expect(changed).toContain('changes-overnight');

    const roles = await page('/roles', DEAD_PULSE);
    expect(roles).toContain('unmeasured');
    expect(roles).not.toMatch(/>0 (principals|actors)/);
    const strip = section(roles, 'trust-strip', '</div>');
    expect(strip).toContain('readings');
    expect(strip).toContain('unmeasured');
  });

  test('pulse answers 500: the HTTP status is the reason shown', async () => {
    fixtures['/api/actor-state'] = { status: 500, body: { error: 'boom' } };
    const working = section(await page('/today'), 'working-now');
    expect(working).toContain('unmeasured');
    expect(working).toContain('500');
    expect(working).not.toContain('0 roles working now');
  });

  test('envelope-level unmeasured entries pass their reason through on every panel', async () => {
    fixtures['/api/actor-state'] = { status: 200, body: { derivedAt: 'x', actors: [], unmeasured: [{ source: 'athena:identity:sessions', reason: 'zz-actor-reason' }] } };
    fixtures['/api/changes-overnight'] = { status: 200, body: { derivedAt: 'x', watermark: null, cards: [], unmeasured: [{ source: 'cards-door', reason: 'zz-changes-reason' }] } };
    fixtures['/api/readings'] = { status: 200, body: { derivedAt: 'x', readings: [], unmeasured: [{ source: 'zz-loom', reason: 'zz-readings-reason' }] } };
    const today = await page('/today');
    const working = section(today, 'working-now');
    expect(working).toContain('unmeasured');
    expect(working).toContain('zz-actor-reason');
    expect(working).not.toContain('0 roles working now');
    const changed = section(today, 'changed-overnight', 'alerts-section');
    expect(changed).toContain('unmeasured');
    expect(changed).toContain('zz-changes-reason');
    const strip = section(today, 'trust-strip', '</div>');
    expect(strip).toContain('zz-loom</span> unmeasured (zz-readings-reason)');
  });

  test('an actor with unmeasured sources shows them under its state', async () => {
    const body = actorStateFixture();
    (body.actors as Json[])[1].stateValue = 'unmeasured';
    (body.actors as Json[])[1].unmeasuredSource = [{ source: 'athena:identity:sessions', reason: 'zz-actor-timeout' }];
    fixtures['/api/actor-state'] = { status: 200, body };
    const roles = await page('/roles');
    expect(roles).toContain('state-word">unmeasured</span>');
    expect(roles).toContain('zz-actor-timeout');
  });
});

describe('e) src derives nothing itself', () => {
  function srcFiles(dir: string): string[] {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
      const p = path.join(dir, e.name);
      return e.isDirectory() ? srcFiles(p) : [p];
    });
  }
  const files = srcFiles(path.join(__dirname, '../src'));

  test('the src tree is non-empty (a missing tree must not pass vacuously)', () => {
    expect(files.length).toBeGreaterThan(5);
  });

  test.each([
    [/loki/i, 'loki'],
    [/count_over_time/, 'count_over_time'],
    [/query_range/, 'query_range'],
    [/deriveRoleState/, 'deriveRoleState'],
    [/changedSince/, 'changedSince'],
  ])('no src file contains %s', (pattern) => {
    const hits = files.filter((f) => pattern.test(fs.readFileSync(f, 'utf8')));
    expect(hits).toEqual([]);
  });
});

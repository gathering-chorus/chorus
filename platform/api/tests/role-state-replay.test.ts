/**
 * @test-type: unit — signal:integration is the fs/tmpdir use, and the only I/O
 * here is a spine fixture this test writes into its own mkdtemp directory and
 * deletes again. No service, no live store, no port. It runs green on a box
 * with the whole stack down.
 *
 * role-state-replay is the tool that answers "what state was each role in at
 * 11:25 yesterday" by replaying the spine through the SAME derivation the live
 * endpoint uses. It shipped with no tests at all (0% of 145 lines), which
 * matters because its two reconstructions — the card column and the sample
 * window — are computed HERE and nowhere else. If they drift, the replay quietly
 * disagrees with the live page and there is nothing to catch it.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  parseArgs, prevDay, toLine, readDay, reconstructWip, sampleRows, renderTable,
  UsageError, type Row,
} from '../src/cli/role-state-replay';
import type { SpineLine } from '../src/derive-role-state';

const ev = (timestamp: string, event: string, role?: string, card_id?: number): SpineLine =>
  ({ timestamp, event, role, card_id } as SpineLine);

// 2026-08-28 is EDT: 11:00 Boston = 15:00Z.
const at = (hm: string) => `2026-08-28T${hm}:00.000Z`;

describe('parseArgs', () => {
  it('defaults the log path to the operator\'s spine', () => {
    const a = parseArgs(['2026-08-28']);
    expect(a.day).toBe('2026-08-28');
    expect(a.logPath).toContain('.chorus/chorus.log');
  });

  it('takes --log from anywhere in the argv, and does not swallow the day', () => {
    const a = parseArgs(['2026-08-28', '--log', '/tmp/spine.log', '11:00']);
    expect(a.logPath).toBe('/tmp/spine.log');
    expect(a.day).toBe('2026-08-28');
    expect(a.samples).toEqual(['11:00']);
  });

  it('falls back to four sample times when none are given', () => {
    expect(parseArgs(['2026-08-28']).samples).toHaveLength(4);
  });

  it.each([[[]], [['yesterday']], [['28-08-2026']], [['--log', '/tmp/x']]])(
    'refuses argv %p instead of replaying a garbage day', (argv) => {
      expect(() => parseArgs(argv as string[])).toThrow(UsageError);
    });
});

describe('prevDay', () => {
  it('steps back one day', () => expect(prevDay('2026-08-28')).toBe('2026-08-27'));
  it('crosses a month boundary', () => expect(prevDay('2026-09-01')).toBe('2026-08-31'));
  it('crosses a year boundary', () => expect(prevDay('2026-01-01')).toBe('2025-12-31'));
});

describe('toLine', () => {
  it('keeps a well-formed spine line', () => {
    expect(toLine({ timestamp: at('15:00'), event: 'card.pulled', role: 'silas', card_id: 4103 }))
      .toMatchObject({ event: 'card.pulled', role: 'silas', card_id: 4103 });
  });

  it('accepts a numeric OR string card id — the spine writes both', () => {
    expect(toLine({ timestamp: at('15:00'), event: 'x', card_id: '77' })?.card_id).toBe('77');
  });

  it.each([
    ['no event', { timestamp: at('15:00') }],
    ['no timestamp', { event: 'card.pulled' }],
    ['event is not a string', { timestamp: at('15:00'), event: 7 }],
  ])('drops a line with %s', (_why, payload) => {
    expect(toLine(payload as Record<string, unknown>)).toBeNull();
  });

  it('drops a role that is not a string rather than carrying it through', () => {
    expect(toLine({ timestamp: at('15:00'), event: 'x', role: 12 })?.role).toBeUndefined();
  });
});

describe('reconstructWip — the card column', () => {
  const now = Date.parse(at('16:00'));

  it('opens a card on card.pulled', () => {
    expect(reconstructWip('silas', [ev(at('15:00'), 'card.pulled', 'silas', 4103)], now))
      .toEqual([{ id: 4103, owner: 'silas' }]);
  });

  it.each(['card.accepted', 'card.unpulled'])('closes it again on %s', (closer) => {
    const events = [ev(at('15:00'), 'card.pulled', 'silas', 4103), ev(at('15:30'), closer, 'silas', 4103)];
    expect(reconstructWip('silas', events, now)).toEqual([]);
  });

  it('ignores another role\'s card', () => {
    expect(reconstructWip('silas', [ev(at('15:00'), 'card.pulled', 'wren', 4101)], now)).toEqual([]);
  });

  it('matches the role case-insensitively — the spine writes both', () => {
    expect(reconstructWip('silas', [ev(at('15:00'), 'card.pulled', 'Silas', 4103)], now))
      .toEqual([{ id: 4103, owner: 'silas' }]);
  });

  it('ignores events AFTER the sample moment — that is the whole point of a replay', () => {
    const later = [ev(at('17:00'), 'card.pulled', 'silas', 4103)];
    expect(reconstructWip('silas', later, now)).toEqual([]);
  });

  it('drops a card pulled more than 24h before the sample', () => {
    const stale = [ev('2026-08-26T15:00:00.000Z', 'card.pulled', 'silas', 4103)];
    expect(reconstructWip('silas', stale, now)).toEqual([]);
  });

  it('reports both when a role really has two cards open', () => {
    const two = [ev(at('15:00'), 'card.pulled', 'silas', 4103), ev(at('15:10'), 'card.pulled', 'silas', 4104)];
    expect(reconstructWip('silas', two, now).map((c) => c.id).sort()).toEqual([4103, 4104]);
  });

  it('ignores a card event with no card id instead of minting NaN', () => {
    expect(reconstructWip('silas', [ev(at('15:00'), 'card.pulled', 'silas')], now)).toEqual([]);
  });
});

describe('readDay', () => {
  let dir: string;
  let log: string;

  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'replay-'));
    log = path.join(dir, 'spine.log');
  });
  afterEach(() => fs.rmSync(dir, { recursive: true, force: true }));

  const write = (lines: string[]) => fs.writeFileSync(log, `${lines.join('\n')}\n`);

  it('keeps the day\'s lines inside the window and drops the rest', async () => {
    write([
      JSON.stringify({ timestamp: at('15:00'), event: 'in-window', role: 'silas' }),
      JSON.stringify({ timestamp: at('23:00'), event: 'after-window', role: 'silas' }),
      JSON.stringify({ timestamp: '2026-08-20T15:00:00.000Z', event: 'other-day', role: 'silas' }),
    ]);
    const kept = await readDay(log, '2026-08-28', Date.parse(at('14:00')), Date.parse(at('16:00')));
    expect(kept.map((e) => e.event)).toEqual(['in-window']);
  });

  it('reads the previous day too — a 24h card lookback crosses midnight', async () => {
    write([JSON.stringify({ timestamp: '2026-08-27T23:30:00.000Z', event: 'card.pulled', role: 'silas', card_id: 1 })]);
    const kept = await readDay(log, '2026-08-28', Date.parse('2026-08-27T00:00:00Z'), Date.parse(at('16:00')));
    expect(kept).toHaveLength(1);
  });

  it('skips a corrupt line instead of dying on the whole replay', async () => {
    write([
      '{ this is not json',
      JSON.stringify({ timestamp: at('15:00'), event: 'survived', role: 'silas' }),
    ]);
    const kept = await readDay(log, '2026-08-28', Date.parse(at('14:00')), Date.parse(at('16:00')));
    expect(kept.map((e) => e.event)).toEqual(['survived']);
  });

  it('skips a line whose timestamp is not a date', async () => {
    write([JSON.stringify({ timestamp: '2026-08-28T25:99:99Z', event: 'bogus-time' })]);
    const kept = await readDay(log, '2026-08-28', 0, Number.MAX_SAFE_INTEGER);
    expect(kept).toEqual([]);
  });
});

describe('sampleRows', () => {
  it('emits one row per role per sample time', () => {
    const rows = sampleRows('2026-08-28', ['11:00', '11:25'], []);
    expect(rows).toHaveLength(6);
    expect(rows.map((r) => r.role)).toEqual(['silas', 'wren', 'kade', 'silas', 'wren', 'kade']);
    expect(rows[0].sample).toBe('2026-08-28 11:00');
  });

  it('shows the card a role had pulled at that moment', () => {
    // 11:25 Boston = 15:25Z
    const events = [
      ev(at('15:00'), 'card.pulled', 'silas', 4103),
      ev(at('15:20'), 'agent.action', 'silas'),
    ];
    const row = sampleRows('2026-08-28', ['11:25'], events).find((r) => r.role === 'silas') as Row;
    expect(row.card).toBe('#4103');
  });

  it('shows an em dash for a role with nothing in flight', () => {
    const row = sampleRows('2026-08-28', ['11:25'], []).find((r) => r.role === 'kade') as Row;
    expect(row.card).toBe('—');
    expect(row.lastEvent).toBe('—');
  });

  it('only feeds the derivation the last hour — an event 90 min old is out of the window', () => {
    const stale = [ev(at('13:50'), 'agent.action', 'silas')];   // 09:50 Boston
    const row = sampleRows('2026-08-28', ['11:25'], stale).find((r) => r.role === 'silas') as Row;
    expect(row.lastEvent).toBe('—');
  });
});

describe('renderTable', () => {
  const rows: Row[] = [
    { sample: '2026-08-28 11:00', role: 'silas', state: 'building', card: '#4103', lastEvent: 'card.pulled', age: '60s ago' },
    { sample: '2026-08-28 11:00', role: 'wren', state: 'idle', card: '—', lastEvent: '—', age: '—' },
  ];

  it('prints a header, a rule and one line per row', () => {
    const lines = renderTable(rows).split('\n');
    expect(lines).toHaveLength(4);
    expect(lines[0]).toContain('role');
    expect(lines[1]).toMatch(/^-+/);
    expect(lines[2]).toContain('silas');
    expect(lines[3]).toContain('wren');
  });

  it('widens a column to its longest value so the table stays aligned', () => {
    const wide = [{ ...rows[0], state: 'blocked-on-a-very-long-reason' }];
    const lines = renderTable(wide).split('\n');
    expect(lines[1].length).toBeGreaterThanOrEqual(lines[0].length - 2);
    expect(lines[2]).toContain('blocked-on-a-very-long-reason');
  });

  it('renders a header even with no rows, rather than empty output', () => {
    expect(renderTable([]).split('\n')[0]).toContain('sample (Boston)');
  });
});

// #2876: emitSpineEvent must coerce numeric-string card_id to integer so the
// log line carries `"card_id":NNN` (unquoted) — the form chorus_logs_for_card
// regex matches. String-form `"card_id":"NNN"` drops out of joins.

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { emitSpineEvent } from '../src/events';

describe('emitSpineEvent card_id canonical type (#2876)', () => {
  const tmpFile = path.join(os.tmpdir(), `cards-card-id-${Date.now()}.log`);

  function emitInProductionMode(extra: Record<string, string | number>): string {
    const origNodeEnv = process.env.NODE_ENV;
    process.env.NODE_ENV = 'production';
    try {
      jest.isolateModules(() => {
        jest.doMock('chorus-sdk', () => ({
          emit: (event: string, role: string, x: Record<string, unknown>) => {
            const captured = { event, role, ...x };
            fs.appendFileSync(tmpFile, JSON.stringify(captured) + '\n');
            return captured;
          },
        }));
        const fresh = require('../src/events') as { emitSpineEvent: typeof emitSpineEvent };
        fresh.emitSpineEvent('card.demo.started', 'silas', extra);
        jest.dontMock('chorus-sdk');
      });
    } finally {
      process.env.NODE_ENV = origNodeEnv;
      jest.resetModules();
    }
    return fs.readFileSync(tmpFile, 'utf-8').trim();
  }

  beforeEach(() => {
    fs.writeFileSync(tmpFile, '');
  });

  afterAll(() => {
    try { fs.unlinkSync(tmpFile); } catch { /* ignore */ }
    jest.resetModules();
  });

  it('emitSpineEvent is a no-op under jest (#2241 guard preserved)', () => {
    emitSpineEvent('card.demo.started', 'silas', { card_id: '2876' });
    expect(fs.readFileSync(tmpFile, 'utf-8')).toBe('');
  });

  it('coerces numeric-string card_id to integer in log line', () => {
    emitSpineEvent('card.demo.started', 'silas', { card_id: '2876' });
    const line = emitInProductionMode({ card_id: '2876', title: 'string-form' });
    expect(line).toMatch(/"card_id":2876\b/);
    expect(line).not.toMatch(/"card_id":"2876"/);
  });

  it('passes integer card_id through unchanged', () => {
    emitSpineEvent('card.demo.started', 'silas', { card_id: 2876 });
    const line = emitInProductionMode({ card_id: 2876, title: 'integer-form' });
    expect(line).toMatch(/"card_id":2876\b/);
  });

  it('leaves non-numeric strings alone (defensive against junk input)', () => {
    emitSpineEvent('card.demo.started', 'silas', { card_id: 'not-a-number' });
    const line = emitInProductionMode({ card_id: 'not-a-number', title: 'edge' });
    expect(line).toMatch(/"card_id":"not-a-number"/);
  });
});

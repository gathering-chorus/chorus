/**
 * context-spine handler tests (#2252).
 */

import {
  fetchContextSpine,
  type ContextSpineDeps,
} from '../../src/handlers/context-spine';

function stubSparql(): ContextSpineDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

const LINES = [
  JSON.stringify({ timestamp: '2026-04-21T10:00:00Z', event: 'card.pulled', role: 'kade', card: '2252', trace_id: 'trace-a' }),
  JSON.stringify({ timestamp: '2026-04-21T10:01:00Z', event: 'card.demo.started', role: 'kade', card: '2252' }),
  JSON.stringify({ timestamp: '2026-04-21T10:02:00Z', event: 'gate.code.passed', role: 'kade', card: '2252', trace_id: 'trace-b' }),
  JSON.stringify({ timestamp: '2026-04-21T10:03:00Z', event: 'gate.quality.passed', role: 'kade', card: '2252' }),
  JSON.stringify({ timestamp: '2026-04-21T10:04:00Z', event: 'card.accepted', role: 'wren', card: '2252' }),
].join('\n');

describe('fetchContextSpine', () => {
  it('returns newest events first, default limit 20', async () => {
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => LINES },
      '/api/chorus/context/spine',
    );
    expect(r.status).toBe(200);
    const body = r.body as { data: { total: number; limit: number; events: Array<{ event: string; timestamp: string }> } };
    expect(body.data.total).toBe(5);
    expect(body.data.limit).toBe(20);
    expect(body.data.events[0].event).toBe('card.accepted');
    expect(body.data.events[4].event).toBe('card.pulled');
  });

  it('honors ?limit=N query param', async () => {
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => LINES },
      '/api/chorus/context/spine?limit=2',
      '2',
    );
    const body = r.body as { data: { total: number; limit: number; events: unknown[] } };
    expect(body.data.total).toBe(2);
    expect(body.data.limit).toBe(2);
    expect(body.data.events).toHaveLength(2);
  });

  it('clamps limit to max 500', async () => {
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => LINES },
      '/api/chorus/context/spine?limit=9999',
      '9999',
    );
    const body = r.body as { data: { limit: number } };
    expect(body.data.limit).toBe(500);
  });

  it('treats invalid limit as default', async () => {
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => LINES },
      '/api/chorus/context/spine?limit=abc',
      'abc',
    );
    const body = r.body as { data: { limit: number } };
    expect(body.data.limit).toBe(20);
  });

  it('preserves trace_id + card when present, omits when absent', async () => {
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => LINES },
      '/api/chorus/context/spine',
    );
    const body = r.body as { data: { events: Array<{ event: string; card?: string; trace_id?: string }> } };
    const pulled = body.data.events.find((e) => e.event === 'card.pulled')!;
    expect(pulled.card).toBe('2252');
    expect(pulled.trace_id).toBe('trace-a');
    const demo = body.data.events.find((e) => e.event === 'card.demo.started')!;
    expect(demo.card).toBe('2252');
    expect(demo.trace_id).toBeUndefined();
  });

  it('skips malformed JSON lines silently', async () => {
    const mixed = [
      'not json',
      JSON.stringify({ timestamp: '2026-04-21T11:00:00Z', event: 'card.pulled', role: 'kade' }),
      '{"partial":',
    ].join('\n');
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => mixed },
      '/api/chorus/context/spine',
    );
    const body = r.body as { data: { total: number; events: Array<{ event: string }> } };
    expect(body.data.total).toBe(1);
    expect(body.data.events[0].event).toBe('card.pulled');
  });

  it('skips entries missing event or timestamp', async () => {
    const mixed = [
      JSON.stringify({ role: 'kade' }),
      JSON.stringify({ event: 'card.pulled', role: 'kade' }),
      JSON.stringify({ timestamp: '2026-04-21T11:00:00Z', event: 'valid', role: 'kade' }),
    ].join('\n');
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => mixed },
      '/api/chorus/context/spine',
    );
    const body = r.body as { data: { total: number } };
    expect(body.data.total).toBe(1);
  });

  it('503 when chorus.log missing', async () => {
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => null },
      '/api/chorus/context/spine',
    );
    expect(r.status).toBe(503);
    expect((r.body as { error: string }).error).toMatch(/No chorus.log/);
  });

  it('envelope carries source + timestamp + domain=chorus', async () => {
    const r = await fetchContextSpine(
      { sparql: stubSparql(), readLog: () => LINES },
      '/api/chorus/context/spine?limit=1',
      '1',
    );
    const body = r.body as { source: string; timestamp: string; domain?: string };
    expect(body.source).toBe('/api/chorus/context/spine?limit=1');
    expect(body.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(body.domain).toBe('chorus');
  });
});

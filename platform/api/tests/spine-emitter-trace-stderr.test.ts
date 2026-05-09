import { createSpineEmitter } from '../src/mcp/server';

// #2845 AC6 — substrate live-emit receipt. Drives the real default stderr
// write path (no captured fake) by spying process.stderr.write, then asserts
// the JSON line emitted via createSpineEmitter("<fixture>") carries the
// fixture trace_id. Confirms closure attaches trace_id at the actual write
// boundary used in production. Loki-side receipt (Promtail → Loki round-trip)
// lands with the first handler-migration card in the #2839 cohort.
describe('createSpineEmitter — live stderr write (#2845 AC6)', () => {
  it('attaches trace_id to JSON written to process.stderr by the default emitter', () => {
    const fixture = '00000000-0000-7000-8000-aaaaaaaaaaaa';
    const event = 'demo.live.emit';
    const writes: string[] = [];
    const orig = process.stderr.write.bind(process.stderr);
    const spy = ((chunk: string | Uint8Array): boolean => {
      writes.push(typeof chunk === 'string' ? chunk : Buffer.from(chunk).toString('utf-8'));
      return true;
    }) as typeof process.stderr.write;
    process.stderr.write = spy;

    try {
      const emit = createSpineEmitter(fixture);
      emit(event, { card_id: 2845 });
    } finally {
      process.stderr.write = orig;
    }

    const line = writes.join('').trim().split('\n').filter(l => l.includes(event)).pop();
    expect(line).toBeDefined();
    const parsed = JSON.parse(line!);
    expect(parsed.event).toBe(event);
    expect(parsed.trace_id).toBe(fixture);
    expect(parsed.card_id).toBe(2845);
    expect(parsed.level).toBe('info');
    expect(typeof parsed.ts).toBe('string');
  });
});

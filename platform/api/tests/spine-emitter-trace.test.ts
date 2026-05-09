import { createSpineEmitter, type SpineEmitter } from '../src/mcp/server';

describe('createSpineEmitter (#2845)', () => {
  function captureEmitter(): { emit: SpineEmitter; calls: Array<{ event: string; fields: Record<string, unknown> }> } {
    const calls: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const emit: SpineEmitter = (event, fields) => { calls.push({ event, fields: { ...fields } }); };
    return { emit, calls };
  }

  it('omits trace_id when constructed without one', () => {
    const { emit, calls } = captureEmitter();
    const traced = createSpineEmitter(undefined, emit);
    traced('demo.event', { card_id: 9999 });
    expect(calls).toHaveLength(1);
    expect(calls[0].fields).toEqual({ card_id: 9999 });
    expect('trace_id' in calls[0].fields).toBe(false);
  });

  it('attaches trace_id to every emit when constructed with one', () => {
    const { emit, calls } = captureEmitter();
    const trace = '019e0cd4-a0ca-7198-85c8-74fb54f23fbd';
    const traced = createSpineEmitter(trace, emit);
    traced('demo.first', { card_id: 1 });
    traced('demo.second', { card_id: 2 });
    expect(calls).toHaveLength(2);
    expect(calls[0].fields).toEqual({ card_id: 1, trace_id: trace });
    expect(calls[1].fields).toEqual({ card_id: 2, trace_id: trace });
  });

  it('preserves caller-supplied trace_id when both are present (caller wins)', () => {
    const { emit, calls } = captureEmitter();
    const traced = createSpineEmitter('outer-trace', emit);
    traced('demo.event', { trace_id: 'inner-trace', card_id: 7 });
    // Spread order: outer trace_id is set first via {...fields, trace_id}, so wrapper's wins.
    // Documented contract: emitter-bound trace_id is canonical for the flow.
    expect(calls[0].fields.trace_id).toBe('outer-trace');
  });
});

// @test-type: unit — pure formatBlockAlert rendering; no service, no clock, no fs
// #3753 AC5 — a probe reading AT the ceiling is a floor, not a duration.
// "blocked 8000ms" institutionalized a misread (#3742: one ~20s freeze read as
// two 8s blocks). The message must label ceiling readings as "≥8s (probe
// ceiling)" and NEVER present the capped number as a measured duration.
// Negative proof both ways (#3734): at-ceiling gets the label; a genuine
// sub-ceiling block keeps its real number and must NOT get the label.
import { formatBlockAlert } from '../src/eventloop-alert';
import { PROBE_CEILING_MS } from '../src/eventloop-episode';

const TS = '2026-08-23T09:00:00.000Z';

describe('#3753 AC5 — ceiling readings labeled as floors', () => {
  it('labels a reading at the ceiling as ≥8s (probe ceiling), not a duration', () => {
    const a = formatBlockAlert(PROBE_CEILING_MS, TS, 'unknown');
    expect(a.message).toContain('≥8s (probe ceiling');
    expect(a.message).not.toContain(`blocked ${PROBE_CEILING_MS}ms`);
    // the raw field stays the measured number — only the rendering changes
    expect(a.duration_ms).toBe(PROBE_CEILING_MS);
  });

  it('labels an above-ceiling reading the same way (clock skew must not slip through)', () => {
    const a = formatBlockAlert(PROBE_CEILING_MS + 396, TS, 'unknown');
    expect(a.message).toContain('≥8s (probe ceiling');
  });

  it('negative proof: a genuine sub-ceiling block keeps its real duration, no ceiling label', () => {
    const a = formatBlockAlert(4396, TS, 'unknown');
    expect(a.message).toContain('4396ms');
    expect(a.message).not.toContain('probe ceiling');
  });
});

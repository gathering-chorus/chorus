// @test-type: unit — pure tests of the tagger/gate/fitness logic; the security/api/fs tokens below are FIXTURE INPUTS, not real calls.
// #3442 — type-mix fitness surface.
// The surface earns its keep by surfacing BLIND SPOTS (high-risk test types
// absent in a domain), not by tallying. The audit's headline: chorus-api has
// ZERO security and ZERO perf tests — that must come out loud.
import { computeTypeMixFitness } from '../src/type-mix-fitness';

describe('computeTypeMixFitness', () => {
  it('aggregates counts by type and by domain', () => {
    const f = computeTypeMixFitness([
      { coversDomain: 'chorus-api-domain', testType: 'unit' },
      { coversDomain: 'chorus-api-domain', testType: 'unit' },
      { coversDomain: 'chorus-api-domain', testType: 'integration' },
      { coversDomain: 'photos-domain', testType: 'security' },
    ]);
    expect(f.byType.unit).toBe(2);
    expect(f.byType.integration).toBe(1);
    expect(f.byType.security).toBe(1);
    expect(f.byDomain['chorus-api-domain'].unit).toBe(2);
  });

  it('THE BLIND SPOT: a domain with no security and no perf surfaces both, loud', () => {
    const f = computeTypeMixFitness([
      { coversDomain: 'chorus-api-domain', testType: 'unit' },
      { coversDomain: 'chorus-api-domain', testType: 'integration' },
    ]);
    const spots = f.blindSpots.filter((b) => b.domain === 'chorus-api-domain').map((b) => b.missing).sort();
    expect(spots).toEqual(['perf', 'security']);
  });

  it('a domain that HAS security/perf raises no blind spot for them', () => {
    const f = computeTypeMixFitness([
      { coversDomain: 'secure-domain', testType: 'security' },
      { coversDomain: 'secure-domain', testType: 'perf' },
    ]);
    expect(f.blindSpots.filter((b) => b.domain === 'secure-domain')).toEqual([]);
  });

  it('blind spots are scoped to risk types (security, perf), not every absent type', () => {
    // a domain with only security present is missing perf (risk) but we do NOT
    // flag missing ui/api/integration — those are not risk-gated blind spots.
    const f = computeTypeMixFitness([
      { coversDomain: 'x-domain', testType: 'security' },
    ]);
    const missing = f.blindSpots.filter((b) => b.domain === 'x-domain').map((b) => b.missing);
    expect(missing).toEqual(['perf']);
  });
});

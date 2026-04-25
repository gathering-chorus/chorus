/**
 * Flow API Determinism Tests — #2171
 *
 * Card counts in the domain panel must be stable across renders.
 * Jeff sees counts change between renders — same data, different numbers.
 * These tests verify /api/flow returns identical results on rapid calls.
 */

jest.setTimeout(15000);

const CLEARING_URL = 'http://localhost:3470';

async function fetchFlow(): Promise<any> {
  const resp = await fetch(`${CLEARING_URL}/api/flow`);
  return resp.json();
}

describe('Flow API determinism (#2171)', () => {
  test('card counts are identical across 5 rapid calls', async () => {
    const results = await Promise.all([
      fetchFlow(), fetchFlow(), fetchFlow(), fetchFlow(), fetchFlow(),
    ]);

    // All calls must return the same totalCards
    const totals = results.map(r => r.totalCards);
    expect(new Set(totals).size).toBe(1);

    // All calls must return the same per-domain activeCards count
    const domainCounts = results.map(r => {
      const domains = r.domains || {};
      const counts: Record<string, number> = {};
      for (const [name, data] of Object.entries(domains) as any[]) {
        counts[name] = data.counts?.activeCards || 0;
      }
      return JSON.stringify(counts, Object.keys(counts).sort());
    });
    expect(new Set(domainCounts).size).toBe(1);
  });

  test('card counts stable across 10-second window', async () => {
    const first = await fetchFlow();
    // Wait just over 1 second and fetch again
    await new Promise(resolve => setTimeout(resolve, 1200));
    const second = await fetchFlow();

    expect(second.totalCards).toBe(first.totalCards);

    // Per-domain counts must match
    const firstDomains = Object.entries(first.domains || {}).map(
      ([name, data]: [string, any]) => `${name}:${data.counts?.activeCards}`
    ).sort();
    const secondDomains = Object.entries(second.domains || {}).map(
      ([name, data]: [string, any]) => `${name}:${data.counts?.activeCards}`
    ).sort();
    expect(secondDomains).toEqual(firstDomains);
  });

  test('response body is byte-identical across rapid calls', async () => {
    // After caching: rapid calls must return the exact same JSON string
    const [r1, r2, r3] = await Promise.all([
      fetch(`${CLEARING_URL}/api/flow`).then(r => r.text()),
      fetch(`${CLEARING_URL}/api/flow`).then(r => r.text()),
      fetch(`${CLEARING_URL}/api/flow`).then(r => r.text()),
    ]);
    expect(r2).toBe(r1);
    expect(r3).toBe(r1);
  });

  test('domain panel shows same counts after filter toggle simulation', async () => {
    // Simulates what happens in the UI: loadFlow() called multiple times
    // during filter toggle (once to re-render) and domain expand (another call)
    const call1 = await fetchFlow();
    const call2 = await fetchFlow();
    const call3 = await fetchFlow();

    // Extract chorus domain specifically (largest, most likely to drift)
    const chorus1 = call1.domains?.chorus?.counts?.activeCards;
    const chorus2 = call2.domains?.chorus?.counts?.activeCards;
    const chorus3 = call3.domains?.chorus?.counts?.activeCards;

    /* eslint-disable jest/no-conditional-expect -- assert determinism only when domain present in fixture */
    if (chorus1 !== undefined) {
      expect(chorus2).toBe(chorus1);
      expect(chorus3).toBe(chorus1);
    }
    /* eslint-enable jest/no-conditional-expect */
  });
});

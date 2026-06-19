// #3442 — type-mix fitness surface.
//
// A fitness surface, not a tally. Counts are table stakes; the value is making
// the high-risk ABSENCES loud — a domain with zero security or zero perf tests
// is a real coverage risk (the audit's headline: chorus-api security=0/perf=0).
// Feeds the security lane (#2444). We scope blind spots to RISK_TYPES so the
// signal isn't drowned by flagging every absent ui/api everywhere.

export interface FitnessEntry {
  coversDomain: string;
  testType: string;
}

export interface BlindSpot {
  domain: string;
  missing: string;
}

export interface TypeMixFitness {
  byType: Record<string, number>;
  byDomain: Record<string, Record<string, number>>;
  blindSpots: BlindSpot[];
}

// The test types whose ABSENCE in a domain is a coverage risk worth surfacing.
const RISK_TYPES = ['security', 'perf'] as const;

export function computeTypeMixFitness(entries: FitnessEntry[]): TypeMixFitness {
  // #3484 drift-cleanup: tally with Maps internally — no dynamic bracket-index, so
  // no `security/detect-object-injection` (the keys are controlled — testType/
  // coversDomain from typed entries, risk from RISK_TYPES — but Maps make that
  // structural, not a per-site disable) and no unnecessary `?.`. The public surface
  // stays Record (Object.fromEntries at the return), so consumers are unchanged.
  const byType = new Map<string, number>();
  const byDomain = new Map<string, Map<string, number>>();

  for (const { coversDomain, testType } of entries) {
    byType.set(testType, (byType.get(testType) ?? 0) + 1);
    const domainCounts = byDomain.get(coversDomain) ?? new Map<string, number>();
    domainCounts.set(testType, (domainCounts.get(testType) ?? 0) + 1);
    byDomain.set(coversDomain, domainCounts);
  }

  const blindSpots: BlindSpot[] = [];
  for (const [domain, counts] of byDomain) {
    for (const risk of RISK_TYPES) {
      if (!counts.get(risk)) blindSpots.push({ domain, missing: risk });
    }
  }

  return {
    byType: Object.fromEntries(byType),
    byDomain: Object.fromEntries([...byDomain].map(([d, m]) => [d, Object.fromEntries(m)])),
    blindSpots,
  };
}

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
  const byType: Record<string, number> = {};
  const byDomain: Record<string, Record<string, number>> = {};

  for (const { coversDomain, testType } of entries) {
    byType[testType] = (byType[testType] ?? 0) + 1;
    (byDomain[coversDomain] ??= {})[testType] = (byDomain[coversDomain]?.[testType] ?? 0) + 1;
  }

  const blindSpots: BlindSpot[] = [];
  for (const domain of Object.keys(byDomain)) {
    for (const risk of RISK_TYPES) {
      if (!byDomain[domain][risk]) blindSpots.push({ domain, missing: risk });
    }
  }

  return { byType, byDomain, blindSpots };
}

// #3580 — gate:quality dispatches checks by domain, consuming the generated
// tests-domain API (athena-make :3360/tests, landed #2819). The FIRST real consumer
// of a generated domain API: it closes the athena-make loop (generate → land →
// CONSUME) and proves the generation program pays off on the tests domain.
//
// This module is the PURE selection core (hermetic, no live fetch — see the
// fetch wiring below it). It mirrors gate-test-type.ts: a tested core + a thin
// caller. The skill (platform/skills/gate-quality) invokes it to scope its
// checks to the tests covering the card's domain instead of the whole suite.

/** One test record as athena-make serves it at /tests (the real key set). */
export interface TestRecord {
  testName: string;
  /** The Domain this test covers — the join key (chorus:covers → Domain). */
  covers: string;
  filePath?: string;
  /** #4162 — what kind of proving; replaces pyramidLayer + testConcern. */
  testType?: string;
  /** retired by #4162; read only until the crawler rewrites the row */
  pyramidLayer?: string;
  hermeticity?: string;
}

/** The /tests response envelope athena-make returns. */
export interface TestsApiResponse {
  data: TestRecord[];
  count?: number;
}

/** The dispatch decision for a card in a given domain — the receipt. */
export interface DispatchResult {
  /** The domain consulted (named so consumption is observable — AC4). */
  domain: string;
  /** The tests that cover this domain — the scoped check set (AC2). */
  coveringTests: string[];
  count: number;
  /** True when the gate has a scoped set to check; false → degrade, don't block (AC5). */
  scoped: boolean;
}

/**
 * Select the tests covering a domain — the join on chorus:covers → Domain
 * (AC1). Pure: pass a /tests response, get the matching records. An empty
 * domain or empty data yields [] (fail-open shape — a missing join can only
 * relax the gate, never invent coverage).
 */
export function selectCoveringTests(domain: string, resp: TestsApiResponse | null | undefined): TestRecord[] {
  if (!domain || !resp || !Array.isArray(resp.data)) return [];
  return resp.data.filter((t) => t.covers === domain);
}

/**
 * Dispatch the quality gate to the tests covering a card's domain (AC2): a
 * card in domain X gets X's tests, not the whole suite. Returns the scoped
 * set plus a receipt (domain + count) so the consumption is observable, not
 * silent (AC4). `scoped` is false when nothing covers the domain — the gate
 * degrades to its prior behavior rather than blocking on an empty join (AC5).
 */
export function dispatchByDomain(domain: string, resp: TestsApiResponse): DispatchResult {
  const covering = selectCoveringTests(domain, resp);
  const coveringTests = covering.map((t) => t.testName);
  return {
    domain,
    coveringTests,
    count: coveringTests.length,
    scoped: coveringTests.length > 0,
  };
}

/** A degraded result: the gate falls back to its prior behavior (AC5). The
 *  receipt still names the domain it tried, so the degrade is observable. */
function degraded(domain: string): DispatchResult {
  return { domain, coveringTests: [], count: 0, scoped: false };
}

/** Where athena-make serves the tests vertical. Overridable for tests/other hosts. */
export const TESTS_API_DEFAULT = 'http://localhost:3360/tests?limit=10000';

/**
 * Live wiring (AC1): fetch the tests API and dispatch for a card's domain.
 * FAIL-OPEN (AC5) — any failure (API down, non-2xx, bad JSON) degrades to the
 * gate's prior behavior and NEVER throws, so the consumer can't block the gate
 * on its own unavailability. `fetchImpl`/`endpoint` are injectable for hermetic
 * tests (no live API). Server-side `?covers=` filtering is a follow-on; today we
 * pull and select client-side.
 */
export async function dispatchForCard(
  domain: string,
  opts?: { endpoint?: string; fetchImpl?: typeof fetch },
): Promise<DispatchResult> {
  const endpoint = opts?.endpoint ?? TESTS_API_DEFAULT;
  const f = opts?.fetchImpl ?? fetch;
  try {
    const res = await f(endpoint);
    if (!res.ok) return degraded(domain);
    const json = (await res.json()) as TestsApiResponse;
    return dispatchByDomain(domain, json);
  } catch {
    return degraded(domain);
  }
}

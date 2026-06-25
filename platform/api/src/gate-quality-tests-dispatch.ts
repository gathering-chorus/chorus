// #3580 — gate:quality dispatches checks by subdomain, consuming the generated
// tests-domain API (owl-api :3360/tests, landed #2819). The FIRST real consumer
// of a generated domain API: it closes the owl-api loop (generate → land →
// CONSUME) and proves the generation program pays off on the tests domain.
//
// This module is the PURE selection core (hermetic, no live fetch — see the
// fetch wiring below it). It mirrors gate-test-type.ts: a tested core + a thin
// caller. The skill (platform/skills/gate-quality) invokes it to scope its
// checks to the tests covering the card's subdomain instead of the whole suite.

/** One test record as owl-api serves it at /tests (the real key set). */
export interface TestRecord {
  testName: string;
  /** The Domain this test covers — the join key (chorus:covers → Domain). */
  covers: string;
  filePath?: string;
  pyramidLayer?: string;
  hermeticity?: string;
}

/** The /tests response envelope owl-api returns. */
export interface TestsApiResponse {
  data: TestRecord[];
  count?: number;
}

/** The dispatch decision for a card in a given subdomain — the receipt. */
export interface DispatchResult {
  /** The subdomain consulted (named so consumption is observable — AC4). */
  subdomain: string;
  /** The tests that cover this subdomain — the scoped check set (AC2). */
  coveringTests: string[];
  count: number;
  /** True when the gate has a scoped set to check; false → degrade, don't block (AC5). */
  scoped: boolean;
}

/**
 * Select the tests covering a subdomain — the join on chorus:covers → Domain
 * (AC1). Pure: pass a /tests response, get the matching records. An empty
 * subdomain or empty data yields [] (fail-open shape — a missing join can only
 * relax the gate, never invent coverage).
 */
export function selectCoveringTests(subdomain: string, resp: TestsApiResponse): TestRecord[] {
  if (!subdomain || !resp || !Array.isArray(resp.data)) return [];
  return resp.data.filter((t) => t.covers === subdomain);
}

/**
 * Dispatch the quality gate to the tests covering a card's subdomain (AC2): a
 * card in subdomain X gets X's tests, not the whole suite. Returns the scoped
 * set plus a receipt (subdomain + count) so the consumption is observable, not
 * silent (AC4). `scoped` is false when nothing covers the subdomain — the gate
 * degrades to its prior behavior rather than blocking on an empty join (AC5).
 */
export function dispatchBySubdomain(subdomain: string, resp: TestsApiResponse): DispatchResult {
  const covering = selectCoveringTests(subdomain, resp);
  const coveringTests = covering.map((t) => t.testName);
  return {
    subdomain,
    coveringTests,
    count: coveringTests.length,
    scoped: coveringTests.length > 0,
  };
}

/** A degraded result: the gate falls back to its prior behavior (AC5). The
 *  receipt still names the subdomain it tried, so the degrade is observable. */
function degraded(subdomain: string): DispatchResult {
  return { subdomain, coveringTests: [], count: 0, scoped: false };
}

/** Where owl-api serves the tests vertical. Overridable for tests/other hosts. */
export const TESTS_API_DEFAULT = 'http://localhost:3360/tests?limit=10000';

/**
 * Live wiring (AC1): fetch the tests API and dispatch for a card's subdomain.
 * FAIL-OPEN (AC5) — any failure (API down, non-2xx, bad JSON) degrades to the
 * gate's prior behavior and NEVER throws, so the consumer can't block the gate
 * on its own unavailability. `fetchImpl`/`endpoint` are injectable for hermetic
 * tests (no live API). Server-side `?covers=` filtering is a follow-on; today we
 * pull and select client-side.
 */
export async function dispatchForCard(
  subdomain: string,
  opts?: { endpoint?: string; fetchImpl?: typeof fetch },
): Promise<DispatchResult> {
  const endpoint = opts?.endpoint ?? TESTS_API_DEFAULT;
  const f = opts?.fetchImpl ?? fetch;
  try {
    const res = await f(endpoint);
    if (!res.ok) return degraded(subdomain);
    const json = (await res.json()) as TestsApiResponse;
    return dispatchBySubdomain(subdomain, json);
  } catch {
    return degraded(subdomain);
  }
}

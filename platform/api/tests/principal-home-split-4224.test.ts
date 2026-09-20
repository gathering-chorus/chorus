// @test-type: unit
/**
 * #4224 — Principals and Permissions do not share a home.
 *
 * On 2026-09-19 the canonical scope query held both classes inside ONE GRAPH
 * clause, and the door swapped that graph for wherever PrincipalShape said
 * Principals lived. The twelve Principal rows moved to
 * urn:chorus:domains:identity, the 46 Permission rows stayed in
 * urn:chorus:domains:security, the join went empty, and every governed write in
 * the system refused — 427 failed crawler writes in four minutes.
 *
 * These are the checks that go red against that shape. Each one is written
 * against the state that broke prod: a principal home that is NOT the security
 * graph.
 */
import * as fs from 'fs';
import * as path from 'path';
import { PRINCIPAL_HOME_MARKER, scopeQueryFor } from '../src/es256-identity';

const CANONICAL_RQ = path.resolve(__dirname, '..', 'src', 'sparql', 'principal-scope.rq');
const TEMPLATE = fs.readFileSync(CANONICAL_RQ, 'utf-8').trim();

const SECURITY = 'urn:chorus:domains:security';
const IDENTITY = 'urn:chorus:domains:identity';

describe('#4224 the scope query reads two graphs', () => {
  test('the shipped template names a marker, never a real graph, for the principal home', () => {
    expect(TEMPLATE).toContain(PRINCIPAL_HOME_MARKER);
    // A marker that looked like a domain graph could silently hold rows, and an
    // un-substituting door would read it instead of failing closed.
    expect(PRINCIPAL_HOME_MARKER.startsWith('urn:chorus:domains:')).toBe(false);
  });

  test('NEGATIVE PROOF: permissions stay in security when the principal home moves', () => {
    const q = scopeQueryFor(TEMPLATE, IDENTITY);
    const sec = q.indexOf(`GRAPH <${SECURITY}>`);
    const home = q.indexOf(`GRAPH <${IDENTITY}>`);
    expect(sec).toBeGreaterThanOrEqual(0);
    expect(home).toBeGreaterThanOrEqual(0);
    expect(sec).not.toEqual(home);

    // Each class under its own graph. Collapse the clauses back into one and
    // one of these two orderings breaks.
    const perm = q.indexOf('chorus:Permission');
    const principal = q.indexOf('chorus:Principal');
    expect(sec).toBeLessThan(perm);
    expect(perm).toBeLessThan(home);
    expect(home).toBeLessThan(principal);
  });

  test('substitution leaves no marker behind, for any home', () => {
    for (const home of [SECURITY, IDENTITY]) {
      expect(scopeQueryFor(TEMPLATE, home)).not.toContain(PRINCIPAL_HOME_MARKER);
    }
  });

  test('the two doors substitute the same marker', () => {
    const rust = fs.readFileSync(
      path.resolve(__dirname, '..', '..', 'services', 'chorus-oidc', 'src', 'oidc.rs'),
      'utf-8',
    );
    expect(rust).toContain(`PRINCIPAL_HOME_MARKER: &str = "${PRINCIPAL_HOME_MARKER}"`);
  });
});

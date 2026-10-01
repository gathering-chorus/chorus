/**
 * @test-type: unit
 * #4415 — completeness never borrows another row's label for a step or owner
 * the domain does not have. On 2026-10-01 board (no primaryStep) reported a
 * TestResult's label as its step, and the meta query took 5.5 s on the live
 * store, because ?step was unbound when `GRAPH ?sg { ?step rdfs:label ?l }`
 * ran and so matched every label in every graph.
 *
 * Runs the real handler and its real query on an oxigraph store.
 */
import { fetchAthenaSubdomainCompleteness } from '../../src/handlers/athena-subdomain-completeness';
import { makeSparqlFromStore } from '../fixtures/oxigraph-sparql';
const oxigraph = require('oxigraph');

const NS = 'https://jeffbridwell.com/chorus#';
const TTL_DOMAINS = `
@prefix chorus: <${NS}> . @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:bare a chorus:Domain ; rdfs:label "bare" ; rdfs:comment "no step, no owner" .
chorus:stepped a chorus:Domain ; rdfs:label "stepped" ; rdfs:comment "has both" ;
  chorus:primaryStep chorus:building ; chorus:ownedBy chorus:principal-kade .
`;
// Unrelated labelled rows in other graphs — what the unbound pattern matched.
const TTL_OTHER = `
@prefix chorus: <${NS}> . @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:testresult-1 rdfs:label "testresult--1790579949661-6929" .
chorus:building rdfs:label "Building" .
chorus:principal-kade rdfs:label "kade (agent principal)" .
`;

function store() {
  const s = new oxigraph.Store();
  s.load(TTL_DOMAINS, { format: 'text/turtle', to_graph_name: oxigraph.namedNode('urn:chorus:domains:domains') });
  s.load(TTL_OTHER, { format: 'text/turtle', to_graph_name: oxigraph.namedNode('urn:chorus:domains:tests') });
  return s;
}

function deps() {
  return { sparqlQuery: makeSparqlFromStore(store()), now: () => 1_000_000 };
}

describe('#4415 completeness reads only the domain\'s own step and owner', () => {
  test('a domain with no step and no owner reports neither', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(), 'bare');
    expect(r.status).toBe(200);
    const data = (r.body as { data: { sections: Record<string, boolean> } }).data;
    expect(data.sections.step).toBe(false);
    expect(data.sections.owner).toBe(false);
  });

  test('a domain with a step and an owner reports their labels', async () => {
    const r = await fetchAthenaSubdomainCompleteness(deps(), 'stepped');
    expect(r.status).toBe(200);
    const data = (r.body as { data: { sections: Record<string, boolean> } }).data;
    expect(data.sections.step).toBe(true);
    expect(data.sections.owner).toBe(true);
  });
});

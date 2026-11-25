// @test-type: unit — upstreams are an in-test fetch; no live athena-make or generated API.
// @domain: principles
// @card: #5034
/**
 * Regression (DEC-2208): the answers of handlers/owl-generated-apis.ts, held to a golden.
 *   - the bind map from the graph rows (which domains have a generated API, and the binds refused)
 *   - the merged discovery document chorus-api serves at /owl/
 * Before #5034 /owl/ was athena-make's discovery as it came, and a class served by a
 * domain's own API (Principle) was missing from it, so the Athena pages said "Not served".
 * A change to either answer's shape must change the golden on purpose.
 */
import * as fs from 'fs';
import * as path from 'path';
import { generatedApisFromRows, mergeDiscovery, type FetchLike } from '../../src/handlers/owl-generated-apis';

const GOLDEN_PATH = path.join(__dirname, 'golden', 'owl-generated-apis.json');
const OWL = 'http://make.test';
const OWN_PORT = 3340;

const domains = [
  { name: 'principles', hasProperty: ['property-principles-server-bind'] },
  { name: 'pulse', hasProperty: ['property-pulse-server-bind'] },
  { name: 'gardens', hasProperty: ['property-gardens-server-bind'] },
  { name: 'decisions' },
];
const properties = [
  { name: 'property-owl-proxy-allowed-hosts', propertyKey: 'owl.proxy.allowed_hosts', propertyValue: '127.0.0.1' },
  { name: 'property-owl-proxy-port-range', propertyKey: 'owl.proxy.port_range', propertyValue: '1024-65535' },
  { name: 'property-principles-server-bind', propertyKey: 'server.bind', propertyValue: '127.0.0.1:3393' },
  { name: 'property-pulse-server-bind', propertyKey: 'server.bind', propertyValue: '127.0.0.1:3394' },
  // refused by the rule: a host outside owl.proxy.allowed_hosts
  { name: 'property-gardens-server-bind', propertyKey: 'server.bind', propertyValue: 'example.com:3395' },
];

/** each upstream URL's answer; a URL not here is a refused connection */
const answers: Record<string, unknown> = {
  [`${OWL}/`]: {
    apiVersion: 'v1', kind: 'Discovery', count: 2,
    primitives: [
      { kind: 'Domain', collection: '/v1/domains/domains', composed: false },
      { kind: 'ADR', collection: '/v1/decisions/adrs', composed: false },
    ],
  },
  'http://127.0.0.1:3393/': {
    apiVersion: 'v1', service: 'principles-api', kind: 'Discovery', count: 2,
    primitives: [
      { kind: 'Principle', collection: '/principles/principles', composed: false },
      // athena-make already lists Domain: not added twice
      { kind: 'Domain', collection: '/principles/domains', composed: false },
    ],
  },
  // 127.0.0.1:3394 (pulse) does not answer: named in `unreachable`
};

const fetchStub: FetchLike = async (url) => {
  if (!(url in answers)) throw new Error(`connect ECONNREFUSED ${url}`);
  return new Response(JSON.stringify(answers[url]), { status: 200, headers: { 'content-type': 'application/json' } });
};

test('owl-generated-apis: the bind map and the merged /owl/ discovery match the golden', async () => {
  const { apis, skipped, ruleProblem } = generatedApisFromRows(domains, properties, OWN_PORT);
  const merged = await mergeDiscovery('/', apis, OWL, fetchStub);
  const got = {
    map: Object.fromEntries(apis),
    skipped,
    ruleProblem,
    discovery: { status: merged.status, contentType: merged.contentType, body: JSON.parse(merged.body) },
  };
  expect(got).toEqual(JSON.parse(fs.readFileSync(GOLDEN_PATH, 'utf-8')));
});

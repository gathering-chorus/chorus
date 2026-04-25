/* global RequestInit */
import { createSparqlSearch, buildSparqlQuery, parseSparqlBindings } from '../src/sparql-search';

function okSparql(bindings: any[]): Response {
  return {
    ok: true,
    status: 200,
    json: async () => ({ results: { bindings } }),
  } as unknown as Response;
}

describe('buildSparqlQuery', () => {
  it('returns empty string when all terms are too short', () => {
    expect(buildSparqlQuery(['a', 'bb'], 10)).toBe('');
  });

  it('includes each term-length-3+ in a CONTAINS filter', () => {
    const q = buildSparqlQuery(['chorus'], 10);
    expect(q).toContain('CONTAINS(LCASE(?text), LCASE(?term0))');
    expect(q).toContain('BIND("chorus" AS ?term0)');
  });

  it('joins multiple terms with &&', () => {
    const q = buildSparqlQuery(['foo', 'bar'], 10);
    expect(q).toContain('?term0');
    expect(q).toContain('?term1');
    expect(q).toContain('&&');
  });

  it('escapes double quotes in term values', () => {
    const q = buildSparqlQuery(['say "hi"'], 10);
    expect(q).toContain('\\"hi\\"');
  });

  it('caps the LIMIT at 50 regardless of input', () => {
    expect(buildSparqlQuery(['chorus'], 200)).toContain('LIMIT 50');
    expect(buildSparqlQuery(['chorus'], 7)).toContain('LIMIT 7');
  });
});

describe('parseSparqlBindings', () => {
  it('returns empty array when bindings is empty', () => {
    expect(parseSparqlBindings([])).toEqual([]);
  });

  it('maps a single binding into the SparqlResult shape', () => {
    const rows = parseSparqlBindings([
      {
        s: { value: 'urn:thing:42' },
        type: { value: 'https://schema.org/Article' },
        domain: { value: 'chorus' },
        label: { value: 'a thing' },
        text: { value: 'a thing with body' },
      },
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toEqual({
      uri: 'urn:thing:42',
      type: 'Article',
      domain: 'chorus',
      label: 'a thing',
      content: 'a thing with body',
      score: 0.5,
    });
  });

  it('falls back to label when text is absent', () => {
    const [row] = parseSparqlBindings([
      { s: { value: 'x' }, type: { value: 'T' }, domain: { value: 'd' }, label: { value: 'only-label' } },
    ]);
    expect(row.content).toBe('only-label');
  });

  it('returns empty strings for missing fields without throwing', () => {
    const [row] = parseSparqlBindings([{}]);
    expect(row.uri).toBe('');
    expect(row.type).toBe('');
    expect(row.label).toBe('');
  });

  it('assigns 0.5 baseline score to every row (RRF merges by rank, not score)', () => {
    const rows = parseSparqlBindings([{}, {}, {}]);
    expect(rows.map(r => r.score)).toEqual([0.5, 0.5, 0.5]);
  });
});

describe('createSparqlSearch', () => {
  it('returns [] when query has no terms of length > 2', async () => {
    const fetchFn = jest.fn();
    const search = createSparqlSearch({ fusekiUrl: 'http://x', fetchFn });
    expect(await search('a bb', 10)).toEqual([]);
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it('calls fetch with a GET to the Fuseki URL when terms qualify', async () => {
    const fetchFn = jest.fn(async () => okSparql([]));
    const search = createSparqlSearch({ fusekiUrl: 'http://fuseki:3030/q', fetchFn });
    await search('chorus', 10);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    const [url, init] = fetchFn.mock.calls[0];
    expect(String(url)).toContain('http://fuseki:3030/q?query=');
    expect((init as RequestInit).headers).toMatchObject({ Accept: 'application/sparql-results+json' });
  });

  it('returns parsed bindings on a successful call', async () => {
    const fetchFn = jest.fn(async () => okSparql([
      { s: { value: 'urn:x' }, type: { value: 'T' }, domain: { value: 'chorus' }, label: { value: 'x' }, text: { value: 'x body' } },
    ]));
    const search = createSparqlSearch({ fusekiUrl: 'http://x', fetchFn });
    const rows = await search('chorus', 10);
    expect(rows).toHaveLength(1);
    expect(rows[0].uri).toBe('urn:x');
  });

  it('returns empty array when response is not ok', async () => {
    const fetchFn = jest.fn(async () => ({ ok: false, status: 500, json: async () => ({}) } as unknown as Response));
    const search = createSparqlSearch({ fusekiUrl: 'http://x', fetchFn });
    expect(await search('chorus', 10)).toEqual([]);
  });

  it('returns empty array when fetch throws (Fuseki down / timeout)', async () => {
    const fetchFn = jest.fn(async () => { throw new Error('down'); });
    const search = createSparqlSearch({ fusekiUrl: 'http://x', fetchFn });
    expect(await search('chorus', 10)).toEqual([]);
  });
});

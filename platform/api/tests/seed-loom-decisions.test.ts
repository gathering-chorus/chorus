import {
  parseDecisions,
  parseAdrFromString,
  buildInsert,
  detectCollisions,
} from '../src/seed-loom-decisions';

describe('parseDecisions', () => {
  it('extracts one row per heading with id uri label date card', () => {
    const md = '## DEC-001: First decision\n- **Date**: 2026-01-15\n- **Card**: #100\nbody\n\n## DEC-2090: Second\n- **Date**: 2026-04-16\n- **Card**: #2090';
    const rows = parseDecisions(md, 'fixture.md');
    expect(rows.length).toBe(2);
    expect(rows[0].id).toBe('dec-001');
    expect(rows[0].uri).toBe('https://jeffbridwell.com/chorus#dec-001');
    expect(rows[0].decisionType).toBe('DEC');
    expect(rows[0].label).toBe('First decision');
    expect(rows[0].date).toBe('2026-01-15');
    expect(rows[0].card).toBe(100);
    expect(rows[1].id).toBe('dec-2090');
    expect(rows[1].card).toBe(2090);
  });

  it('returns empty array when no headings present', () => {
    expect(parseDecisions('# unrelated\nno headings', 'x.md')).toEqual([]);
  });

  it('omits date and card when those lines are missing', () => {
    const rows = parseDecisions('## DEC-005: Sparse\nbody only', 'x.md');
    expect(rows[0].date).toBeUndefined();
    expect(rows[0].card).toBeUndefined();
  });
});

describe('parseAdrFromString', () => {
  it('extracts ADR-026 fields from a representative file', () => {
    const adr = '# ADR-026: CI architecture and lock-file policy\n\n**Status:** Accepted\n**Card:** unblocks #2481\n\n## Context\nbody';
    const row = parseAdrFromString(adr, 'ADR-026.md');
    expect(row).not.toBeNull();
    expect(row && row.id).toBe('adr-026');
    expect(row && row.decisionType).toBe('ADR');
    expect(row && row.label).toBe('CI architecture and lock-file policy');
    expect(row && row.status).toBe('Accepted');
    expect(row && row.card).toBe(2481);
  });

  it('parses Supersedes when it references another ADR by number', () => {
    const adr = '# ADR-010: New approach\n**Status:** Accepted\n**Supersedes:** ADR-007 was the old one';
    const row = parseAdrFromString(adr, 'ADR-010.md');
    expect(row && row.supersedes).toBe('https://jeffbridwell.com/chorus#adr-007');
  });

  it('returns null for files lacking an ADR heading', () => {
    expect(parseAdrFromString('# Random doc\nno adr', 'x.md')).toBeNull();
  });
});

describe('detectCollisions', () => {
  it('returns null when all URIs are unique', () => {
    const rows = [
      { id: 'dec-001', uri: 'u1', decisionType: 'DEC' as const, label: 'a', body: '', source: 'a' },
      { id: 'dec-002', uri: 'u2', decisionType: 'DEC' as const, label: 'b', body: '', source: 'b' },
    ];
    expect(detectCollisions(rows)).toBeNull();
  });

  it('reports the duplicate URI and both source paths', () => {
    const rows = [
      { id: 'dec-093', uri: 'u', decisionType: 'DEC' as const, label: 'a', body: '', source: 'decisions.md' },
      { id: 'dec-093', uri: 'u', decisionType: 'DEC' as const, label: 'b', body: '', source: 'adr/DEC-093.md' },
    ];
    const collision = detectCollisions(rows);
    expect(collision).not.toBeNull();
    expect(collision && collision.uri).toBe('u');
    expect(collision && collision.sources).toEqual(['decisions.md', 'adr/DEC-093.md']);
  });
});

describe('buildInsert', () => {
  it('emits SPARQL INSERT DATA targeting urn:chorus:instances graph', () => {
    const rows = [
      {
        id: 'dec-001',
        uri: 'https://jeffbridwell.com/chorus#dec-001',
        decisionType: 'DEC' as const,
        label: 'First',
        body: 'body text',
        date: '2026-01-15',
        card: 100,
        source: 'x',
      },
    ];
    const sparql = buildInsert(rows);
    expect(sparql.includes('INSERT DATA')).toBe(true);
    expect(sparql.includes('GRAPH <urn:chorus:instances>')).toBe(true);
    expect(sparql.includes('a chorus:Decision')).toBe(true);
    expect(sparql.includes('chorus:decisionType "DEC"')).toBe(true);
    expect(sparql.includes('dcterms:created "2026-01-15"')).toBe(true);
    expect(sparql.includes('chorus:relatedCard "100"')).toBe(true);
  });

  it('escapes quotes and newlines in rdfs:comment bodies', () => {
    const rows = [
      {
        id: 'dec-001',
        uri: 'https://jeffbridwell.com/chorus#dec-001',
        decisionType: 'DEC' as const,
        label: 'L',
        body: 'has "quotes" and\nnewlines',
        source: 'x',
      },
    ];
    const sparql = buildInsert(rows);
    expect(sparql.includes('\\"quotes\\"')).toBe(true);
    expect(sparql.includes('\\n')).toBe(true);
  });

  it('omits optional predicates when fields are absent', () => {
    const rows = [
      {
        id: 'dec-001',
        uri: 'https://jeffbridwell.com/chorus#dec-001',
        decisionType: 'DEC' as const,
        label: 'L',
        body: 'b',
        source: 'x',
      },
    ];
    const sparql = buildInsert(rows);
    expect(sparql.includes('dcterms:created')).toBe(false);
    expect(sparql.includes('chorus:status')).toBe(false);
    expect(sparql.includes('chorus:relatedCard')).toBe(false);
    expect(sparql.includes('chorus:supersedes')).toBe(false);
  });
});

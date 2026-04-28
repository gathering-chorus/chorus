// #2516 — Tests for the alias-graph migration's pure logic.
// Verifies deriveAliases produces the canonical triple set that
// preserves current buildAliasMap behavior at migration time.

import { deriveAliases } from '../scripts/migrate-aliases-to-graph';

describe('deriveAliases', () => {
  it('skips generic-base SubDomains', () => {
    const pairs = deriveAliases([
      { id: 'code-domain', label: 'code' },
      { id: 'time-domain', label: 'time' },
      { id: 'photos-domain', label: 'photos' },
    ]);
    const aliases = pairs.map(([a]) => a);
    expect(aliases).not.toContain('code');
    expect(aliases).not.toContain('time');
    expect(aliases).toContain('photos');
  });

  it('emits singular alias for plural -s subdomains', () => {
    const pairs = deriveAliases([{ id: 'photos-domain', label: 'photos' }]);
    expect(pairs).toContainEqual(['photo', 'photos-domain']);
    expect(pairs).toContainEqual(['photos', 'photos-domain']);
  });

  it('handles -ies → -y pluralization', () => {
    const pairs = deriveAliases([{ id: 'stories-domain', label: 'stories' }]);
    expect(pairs).toContainEqual(['story', 'stories-domain']);
    expect(pairs).toContainEqual(['stories', 'stories-domain']);
  });

  it('emits the 5 SPECIAL_ALIASES even with empty input', () => {
    const pairs = deriveAliases([]);
    expect(pairs).toContainEqual(['wordpress', 'blog-domain']);
    expect(pairs).toContainEqual(['socialpost', 'social-domain']);
    expect(pairs).toContainEqual(['sms-seed', 'seeds-domain']);
    expect(pairs).toContainEqual(['self-ai', 'sexuality-domain']);
    expect(pairs).toContainEqual(['ontology', 'convergence-domain']);
    expect(pairs).toHaveLength(5);
  });

  it('preserves full hyphenated id as alias for compound names', () => {
    const pairs = deriveAliases([
      { id: 'loom-policies', label: 'loom policies' },
      { id: 'alerts-monitors-domain', label: 'alerts and monitors' },
    ]);
    expect(pairs).toContainEqual(['loom-policies', 'loom-policies']);
    expect(pairs).toContainEqual(['loom-policy', 'loom-policies']);
    expect(pairs).toContainEqual(['alerts-monitors', 'alerts-monitors-domain']);
    expect(pairs).toContainEqual(['alerts-monitor', 'alerts-monitors-domain']);
  });

  it('emits both triples on properties/property collision (last-write-wins resolved by query ORDER BY)', () => {
    const pairs = deriveAliases([
      { id: 'properties-domain', label: 'properties' },
      { id: 'property-domain', label: 'property' },
    ]);
    const propertyPairs = pairs.filter(([a]) => a === 'property');
    expect(propertyPairs).toHaveLength(2);
    expect(propertyPairs).toContainEqual(['property', 'properties-domain']);
    expect(propertyPairs).toContainEqual(['property', 'property-domain']);
  });

  it('produces 72 pairs against the current 48-subdomain set (golden count)', () => {
    // Lightweight golden — exact list is tested via spot-checks above; here we
    // pin the total to catch silent drift if a SubDomain adds/removes.
    const fixture = [
      'alerts-monitors-domain', 'athena-domain', 'blog-domain', 'books-domain',
      'cards-service', 'chorus-domain', 'code-domain', 'commits-domain',
      'convergence-domain', 'deploys-domain', 'documents-domain', 'gates-service',
      'heralds-domain', 'infrastructure-domain', 'integrations-domain',
      'knowledge-domain', 'logs-domain', 'loom-analytics', 'loom-decisions',
      'loom-metrics', 'loom-policies', 'loom-practices', 'loom-principles',
      'loom-rcas', 'messages-domain', 'music-domain', 'notes-domain',
      'observability-domain', 'people-domain', 'photos-domain', 'pipelines-domain',
      'policies-domain', 'properties-domain', 'property-domain', 'roles-domain',
      'security-domain', 'seeds-domain', 'services-domain', 'sexuality-domain',
      'skills-service', 'social-domain', 'spine-service', 'stories-domain',
      'streams-domain', 'tests-domain', 'time-domain', 'toolchain-domain',
      'video-domain',
    ].map((id) => ({ id, label: id }));
    expect(fixture).toHaveLength(48);
    const pairs = deriveAliases(fixture);
    expect(pairs).toHaveLength(72);
  });
});

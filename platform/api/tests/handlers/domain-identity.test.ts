// @test-type: unit — pure functions over name tables; no store, no network.
// @card: #4353
// @owner: wren
/**
 * domain-identity.test.ts — #2430
 * What Jeff sees: one shared resolver, every domain fold filters
 * consistently. These tests pin the contract.
 */

import { describe, it, expect } from '@jest/globals';
import { resolveDomainIdentity, cardDomainSearchLabels } from '../../src/handlers/domain-identity';

describe('resolveDomainIdentity — normalization', () => {
  it('accepts kebab input unchanged', () => {
    expect(resolveDomainIdentity('principles').primary).toBe('principles');
  });

  it('normalizes underscore to kebab', () => {
    expect(resolveDomainIdentity('version_control').primary).toBe('version-control');
  });

  it('strips -domain suffix', () => {
    expect(resolveDomainIdentity('chorus-domain').primary).toBe('chorus');
    expect(resolveDomainIdentity('tests').primary).toBe('tests');
  });

  it('does NOT strip -service / -analytics / other words — only -domain', () => {
    // loom-analytics is a real domain, not a "loom with analytics suffix"
    expect(resolveDomainIdentity('analytics').primary).toBe('analytics');
    // pulse-service is its own domain id — don't collapse to 'pulse'
    expect(resolveDomainIdentity('pulse-service').primary).toBe('pulse-service');
  });

  it('lowercases mixed case input', () => {
    expect(resolveDomainIdentity('Principles').primary).toBe('principles');
  });
});

describe('resolveDomainIdentity — loom sub-domains fold into loom parent', () => {
  it('loom-principles cards match sequence:loom (parent tag)', () => {
    const id = resolveDomainIdentity('principles');
    expect(id.aliases).toContain('loom');
    expect(id.cardSequenceTags).toContain('loom');
  });

  it('all 7 loom sub-domains alias to loom', () => {
    const subs = ['principles', 'policies', 'practices', 'decisions', 'metrics', 'analytics', 'rcas'];
    for (const s of subs) {
      const id = resolveDomainIdentity(s);
      expect(id.aliases).toContain('loom');
    }
  });

  it('principles-domain (with suffix) resolves identically to principles', () => {
    const a = resolveDomainIdentity('principles-domain');
    const b = resolveDomainIdentity('principles');
    expect(a.primary).toBe(b.primary);
    expect(a.aliases).toEqual(b.aliases);
  });
});

describe('resolveDomainIdentity — special cases (tests/code/gates)', () => {
  it('tests domain aliases to quality', () => {
    const id = resolveDomainIdentity('tests');
    expect(id.primary).toBe('tests');
    expect(id.aliases).toContain('quality');
  });

  it('code domain aliases to code', () => {
    const id = resolveDomainIdentity('code');
    expect(id.aliases).toContain('code');
  });

  it('gates domain aliases to gates', () => {
    const id = resolveDomainIdentity('gates');
    expect(id.aliases).toContain('gates');
  });
});

describe('resolveDomainIdentity — default behavior for unregistered domains', () => {
  it('returns the normalized id as the primary card domain tag by default', () => {
    const id = resolveDomainIdentity('seeds');
    expect(id.primary).toBe('seeds');
    expect(id.cardDomainTags).toEqual(['seeds']);
    expect(id.aliases).toEqual([]);
  });

  it('derives alert tokens from hyphenated id', () => {
    const id = resolveDomainIdentity('photos-ingest');
    expect(id.alertFileTokens).toEqual(['photos', 'ingest']);
  });

  it('defaults ontologyGraph to urn:chorus:ontology', () => {
    const id = resolveDomainIdentity('chorus-domain');
    expect(id.ontologyGraph).toBe('urn:chorus:ontology');
  });

  it('builds domainUri from chorus# namespace', () => {
    const id = resolveDomainIdentity('principles');
    expect(id.domainUri).toBe('https://jeffbridwell.com/chorus#principles');
  });
});

describe('cardDomainSearchLabels helper', () => {
  it('returns primary + aliases for card-search handlers', () => {
    const id = resolveDomainIdentity('principles');
    const labels = cardDomainSearchLabels(id);
    expect(labels).toContain('principles');
    expect(labels).toContain('loom');
  });

  it('returns just primary for unregistered domain', () => {
    const id = resolveDomainIdentity('seeds');
    const labels = cardDomainSearchLabels(id);
    expect(labels).toEqual(['seeds']);
  });
});

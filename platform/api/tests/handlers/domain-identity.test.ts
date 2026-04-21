/**
 * domain-identity.test.ts — #2430
 * What Jeff sees: one shared resolver, every subdomain fold filters
 * consistently. These tests pin the contract.
 */

import { describe, it, expect } from '@jest/globals';
import { resolveDomainIdentity, cardDomainSearchLabels } from '../../src/handlers/domain-identity';

describe('resolveDomainIdentity — normalization', () => {
  it('accepts kebab input unchanged', () => {
    expect(resolveDomainIdentity('loom-principles').primary).toBe('loom-principles');
  });

  it('normalizes underscore to kebab', () => {
    expect(resolveDomainIdentity('loom_principles').primary).toBe('loom-principles');
  });

  it('strips -domain suffix', () => {
    expect(resolveDomainIdentity('chorus-domain').primary).toBe('chorus');
    expect(resolveDomainIdentity('tests-domain').primary).toBe('tests');
  });

  it('does NOT strip -service / -analytics / other words — only -domain', () => {
    // loom-analytics is a real subdomain, not a "loom with analytics suffix"
    expect(resolveDomainIdentity('loom-analytics').primary).toBe('loom-analytics');
    // pulse-service is its own subdomain id — don't collapse to 'pulse'
    expect(resolveDomainIdentity('pulse-service').primary).toBe('pulse-service');
  });

  it('lowercases mixed case input', () => {
    expect(resolveDomainIdentity('Loom-Principles').primary).toBe('loom-principles');
  });
});

describe('resolveDomainIdentity — loom sub-subdomains fold into loom parent', () => {
  it('loom-principles cards match sequence:loom (parent tag)', () => {
    const id = resolveDomainIdentity('loom-principles');
    expect(id.aliases).toContain('loom');
    expect(id.cardSequenceTags).toContain('loom');
  });

  it('all 7 loom sub-subdomains alias to loom', () => {
    const subs = ['loom-principles', 'loom-policies', 'loom-practices', 'loom-decisions', 'loom-metrics', 'loom-analytics', 'loom-rcas'];
    for (const s of subs) {
      const id = resolveDomainIdentity(s);
      expect(id.aliases).toContain('loom');
    }
  });

  it('loom-principles-domain (with suffix) resolves identically to loom-principles', () => {
    const a = resolveDomainIdentity('loom-principles-domain');
    const b = resolveDomainIdentity('loom-principles');
    expect(a.primary).toBe(b.primary);
    expect(a.aliases).toEqual(b.aliases);
  });
});

describe('resolveDomainIdentity — special cases (tests/code/gates)', () => {
  it('tests subdomain aliases to quality', () => {
    const id = resolveDomainIdentity('tests-domain');
    expect(id.primary).toBe('tests');
    expect(id.aliases).toContain('quality');
  });

  it('code subdomain aliases to code', () => {
    const id = resolveDomainIdentity('code-domain');
    expect(id.aliases).toContain('code');
  });

  it('gates subdomain aliases to gates', () => {
    const id = resolveDomainIdentity('gates-domain');
    expect(id.aliases).toContain('gates');
  });
});

describe('resolveDomainIdentity — default behavior for unregistered subdomains', () => {
  it('returns the normalized id as the primary card domain tag by default', () => {
    const id = resolveDomainIdentity('seeds-domain');
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

  it('builds subdomainUri from chorus# namespace', () => {
    const id = resolveDomainIdentity('loom-principles');
    expect(id.subdomainUri).toBe('https://jeffbridwell.com/chorus#loom-principles');
  });
});

describe('cardDomainSearchLabels helper', () => {
  it('returns primary + aliases for card-search handlers', () => {
    const id = resolveDomainIdentity('loom-principles');
    const labels = cardDomainSearchLabels(id);
    expect(labels).toContain('loom-principles');
    expect(labels).toContain('loom');
  });

  it('returns just primary for unregistered subdomain', () => {
    const id = resolveDomainIdentity('seeds-domain');
    const labels = cardDomainSearchLabels(id);
    expect(labels).toEqual(['seeds']);
  });
});

/**
 * doc-tag-drift unit tests (#2520 AC6).
 */
import { detectDrift, type DocWithTags } from '../../src/handlers/doc-tag-drift';

const VALID = ['loom-decisions', 'loom-principles', 'athena-domain', 'photos-domain'];

function doc(href: string, subdomain?: string): DocWithTags {
  return {
    href, source: 'test', title: href,
    tags: { confidence: 'high', signal: 'path', subdomain },
  };
}

describe('detectDrift (#2520 AC6)', () => {
  test('valid subdomains produce no drift', () => {
    const r = detectDrift([doc('/a', 'loom-decisions'), doc('/b', 'photos-domain')], VALID);
    expect(r).toEqual([]);
  });

  test('unknown subdomain surfaces with closest match', () => {
    const r = detectDrift([doc('/x', 'loom-decision')], VALID);
    expect(r.length).toBe(1);
    expect(r[0].claimedSubdomain).toBe('loom-decision');
    expect(r[0].closestMatch).toBe('loom-decisions');
  });

  test('docs without subdomain skipped (no false drift)', () => {
    const r = detectDrift([doc('/a'), doc('/b')], VALID);
    expect(r).toEqual([]);
  });

  test('far-off claim returns no closestMatch', () => {
    const r = detectDrift([doc('/x', 'totally-unrelated-name')], VALID);
    expect(r.length).toBe(1);
    expect(r[0].closestMatch).toBeUndefined();
  });
});

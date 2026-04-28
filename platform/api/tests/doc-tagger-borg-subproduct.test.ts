/**
 * Tagger: Borg is a subproduct of Chorus, not a top-level product.
 *
 * Verifies the ontology change landed 2026-04-28 (Athena: chorus:borgProduct
 * type → SubProduct, chorusProduct hasSubProduct borgProduct) propagates
 * through the tagger so a borg-* filename returns product=chorus, subproduct=borg
 * (not product=borg, not subproduct=undefined).
 */

import { inferTags } from '../src/handlers/doc-tagger';

describe('doc-tagger — Borg as Chorus subproduct (2026-04-28 ontology shift)', () => {
  test('borg-prefixed filename returns product=chorus, subproduct=borg', () => {
    const r = inferTags({ sourcePath: '/some/path', basename: 'borg-handler-error.html' });
    expect(r.product).toBe('chorus');
    expect(r.subproduct).toBe('borg');
  });

  test('embedded -borg- token in filename returns product=chorus, subproduct=borg', () => {
    const r = inferTags({ sourcePath: '/x', basename: 'service-borg-design.md' });
    expect(r.product).toBe('chorus');
    expect(r.subproduct).toBe('borg');
  });
});

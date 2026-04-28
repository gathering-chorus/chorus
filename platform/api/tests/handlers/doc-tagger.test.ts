/**
 * doc-tagger unit tests (#2520).
 *
 * Inference rules: filename + path + content keywords → product/subproduct/
 * domain/subdomain candidates against Athena ontology. Tests describe what
 * Jeff sees: every tag assignment is justifiable from filename or path
 * alone for the high-confidence cases; content is only consulted when
 * filename + path are inconclusive.
 */
import {
  inferTags,
  type DocTagInput,
  type DocTags,
} from '../../src/handlers/doc-tagger';

describe('doc-tagger.inferTags (#2520)', () => {
  test('ADR-NNN file → Chorus product, Loom subproduct, loom-decisions subdomain', () => {
    const r: DocTags = inferTags({
      sourcePath: 'roles/silas/adr/ADR-014-pod-mediated-coordination.md',
      basename: 'ADR-014-pod-mediated-coordination.md',
    });
    expect(r.product).toBe('chorus');
    expect(r.subproduct).toBe('loom');
    expect(r.subdomain).toBe('loom-decisions');
    expect(r.confidence).toBe('high');
    expect(r.signal).toBe('path');
  });

  test('chorus-er-diagram.html in designing/docs → Chorus, Athena subproduct', () => {
    const r = inferTags({
      sourcePath: 'designing/docs/chorus-er-diagram.html',
      basename: 'chorus-er-diagram.html',
    });
    expect(r.product).toBe('chorus');
    expect(r.subproduct).toBe('athena');
    expect(r.confidence).toBeDefined();
  });

  test('domain-blog.html → Gathering product, blog-domain subdomain (no subproduct)', () => {
    const r = inferTags({
      sourcePath: 'public/gathering-docs/domain-blog.html',
      basename: 'domain-blog.html',
    });
    expect(r.product).toBe('gathering');
    expect(r.subproduct).toBeUndefined();
    expect(r.subdomain).toBe('blog-domain');
  });

  test('akasha/* path → Consulting product (no subproduct, no subdomain)', () => {
    // Akasha rolled into Consulting per 2026-04-28 ontology shift.
    const r = inferTags({
      sourcePath: 'public/akasha/practice-1.md',
      basename: 'practice-1.md',
    });
    expect(r.product).toBe('consulting');
    expect(r.subproduct).toBeUndefined();
  });

  test('werk-card-lifecycle.html → Chorus, Werk subproduct', () => {
    const r = inferTags({
      sourcePath: 'designing/docs/werk-card-lifecycle.html',
      basename: 'werk-card-lifecycle.html',
    });
    expect(r.product).toBe('chorus');
    expect(r.subproduct).toBe('werk');
  });

  test('ATTENTION_ARCHITECTURE.html → Chorus product (content-token UPPERCASE match)', () => {
    const r = inferTags({
      sourcePath: 'designing/docs/ATTENTION_ARCHITECTURE.html',
      basename: 'ATTENTION_ARCHITECTURE.html',
    });
    expect(r.product).toBe('chorus');
  });

  test('domain-photos.html → Gathering, photos-domain', () => {
    const r = inferTags({
      sourcePath: 'public/gathering-docs/domain-photos.html',
      basename: 'domain-photos.html',
    });
    expect(r.product).toBe('gathering');
    expect(r.subdomain).toBe('photos-domain');
  });

  test('frontmatter override beats inference', () => {
    const r = inferTags({
      sourcePath: 'designing/docs/ambiguous.md',
      basename: 'ambiguous.md',
      frontmatter: { product: 'chorus', subproduct: 'borg', subdomain: 'borg-domain' },
    });
    expect(r.product).toBe('chorus');
    expect(r.subproduct).toBe('borg');
    expect(r.subdomain).toBe('borg-domain');
    expect(r.signal).toBe('frontmatter');
  });

  test('no signal at all → product=null, confidence=none', () => {
    const r = inferTags({
      sourcePath: 'tmp/random.md',
      basename: 'random.md',
    });
    expect(r.confidence).toBe('none');
  });
});

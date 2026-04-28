/**
 * Tagger: Akasha-classified content rolls up to product=consulting.
 *
 * Akasha was emitted as its own product enum but Athena had no Akasha Product
 * instance. Per Jeff (2026-04-28), Akasha is one strand of his Consulting
 * product line — fold akasha/* path classification into product=consulting.
 */

import { inferTags } from '../src/handlers/doc-tagger';

describe('doc-tagger — Akasha → Consulting (2026-04-28)', () => {
  test('akasha/ path returns product=consulting', () => {
    const r = inferTags({ sourcePath: 'akasha/proposal.html', basename: 'proposal.html' });
    expect(r.product).toBe('consulting');
  });

  test('embedded /akasha/ path returns product=consulting', () => {
    const r = inferTags({ sourcePath: 'docs/akasha/value-stream.html', basename: 'value-stream.html' });
    expect(r.product).toBe('consulting');
  });
});

// @test-type: unit — pure classifier, fixture inputs only; no fs, no services.
import { inferTags } from '../src/handlers/doc-tagger';

// --- #3606 — cover the tagsFromPath cabinet branches + content fallback +
// frontmatter precedence that were the file's uncovered remainder.
describe('inferTags — path cabinets (#3606)', () => {
  const t = (sourcePath: string, basename = 'x.md') => inferTags({ sourcePath, basename });

  it('ADR paths → chorus/loom/loom-decisions', () => {
    expect(t('roles/silas/adr/ADR-001.md')).toMatchObject({ product: 'chorus', subproduct: 'loom', subdomain: 'loom-decisions', signal: 'path' });
  });

  it('akasha → consulting; gathering-docs → gathering; chorus-docs → chorus', () => {
    expect(t('public/akasha/pitch.html')).toMatchObject({ product: 'consulting' });
    expect(t('public/gathering-docs/plan.html')).toMatchObject({ product: 'gathering' });
    expect(t('public/chorus-docs/arch.html')).toMatchObject({ product: 'chorus' });
  });

  it('decisions cabinets → loom-decisions; designing/docs + role dirs + docs/ → chorus', () => {
    expect(t('designing/decisions/DEC-9.md')).toMatchObject({ subdomain: 'loom-decisions' });
    expect(t('designing/docs/whatever.html')).toMatchObject({ product: 'chorus' });
    expect(t('roles/kade/notes.md')).toMatchObject({ product: 'chorus' });
    expect(t('docs/diagrams/c4.md')).toMatchObject({ product: 'chorus' });
  });

  it('manual loom hrefs → chorus/loom; bare public/ root files → gathering medium', () => {
    expect(t('manual/loom/principles.html')).toMatchObject({ product: 'chorus', subproduct: 'loom' });
    expect(t('public/business-plan.html')).toMatchObject({ product: 'gathering', confidence: 'medium' });
  });

  it('athena enrichment: chorus doc with ontology-ish basename gains subproduct athena', () => {
    expect(t('designing/docs/er-diagram-full.html', 'er-diagram-full.html')).toMatchObject({ product: 'chorus', subproduct: 'athena' });
  });
});

describe('inferTags — frontmatter beats path; content is last resort (#3606)', () => {
  it('frontmatter wins outright', () => {
    const r = inferTags({ sourcePath: 'public/gathering-docs/x.md', basename: 'x.md', frontmatter: { product: 'chorus', subproduct: 'werk' } });
    expect(r).toMatchObject({ product: 'chorus', subproduct: 'werk', signal: 'frontmatter', confidence: 'high' });
  });

  it('content keywords tag an otherwise-signalless doc, low confidence', () => {
    const chorusHead = 'chorus werk loom athena chorus werk kade silas';
    expect(inferTags({ sourcePath: 'ZZZ-nowhere', basename: 'x.md', contentHead: chorusHead })).toMatchObject({ product: 'chorus', confidence: 'low', signal: 'content' });
    const gatherHead = 'garden photo blog gathering garden photo';
    expect(inferTags({ sourcePath: 'ZZZ-nowhere', basename: 'x.md', contentHead: gatherHead })).toMatchObject({ product: 'gathering', confidence: 'low' });
  });

  it('ambiguous content stays untagged', () => {
    expect(inferTags({ sourcePath: 'ZZZ-nowhere', basename: 'x.md', contentHead: 'chorus gathering chorus gathering chorus gathering' })).toMatchObject({ confidence: 'none', signal: 'none' });
  });
});

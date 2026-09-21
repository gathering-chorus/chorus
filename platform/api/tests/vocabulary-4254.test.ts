// @test-type: unit — pure fold over fixture SPARQL bindings; no service, no store, no network
// #4254 — the fold from SPARQL rows to the vocabulary page's view.
//
// The rows arrive one per OPTIONAL combination, so a concept with two
// altLabels arrives twice and must merge into one term, never two.

import { buildVocabulary, type VocabBinding } from '../src/handlers/vocabulary';

const v = (value: string) => ({ value });

const row = (o: Partial<Record<keyof VocabBinding, string>>): VocabBinding =>
  Object.fromEntries(Object.entries(o).map(([k, s]) => [k, v(s as string)])) as VocabBinding;

const SERVICE = {
  concept: 'urn:chorus:domains:vocabulary#service',
  scheme: 'urn:chorus:domains:vocabulary#identity',
  schemeLabel: 'Chorus identity vocabulary',
  pref: 'service',
  def: 'A process we run that holds a credential.',
  match: 'https://jeffbridwell.com/chorus#principalKind',
};

describe('buildVocabulary', () => {
  it('merges the rows of one concept into one term', () => {
    const out = buildVocabulary([
      row({ ...SERVICE, alt: 'worker' }),
      row({ ...SERVICE, alt: 'automation' }),
    ]);
    expect(out.termCount).toBe(1);
    const term = out.schemes[0].terms[0];
    expect(term.prefLabel).toBe('service');
    expect(term.altLabels).toEqual(['automation', 'worker']);
    expect(term.exactMatch).toBe('principalKind');
  });

  it('keeps one word in two schemes as two terms', () => {
    // The homonym. Folding these together would answer "what is an agent?"
    // with one of the two meanings and hide the other.
    const out = buildVocabulary([
      row({
        concept: 'urn:chorus:domains:vocabulary#agent',
        scheme: 'urn:chorus:domains:vocabulary#identity',
        pref: 'agent',
      }),
      row({
        concept: 'urn:chorus:domains:vocabulary#launch-agent',
        scheme: 'urn:chorus:domains:vocabulary#runtime',
        pref: 'agent',
      }),
    ]);
    expect(out.termCount).toBe(2);
    expect(out.schemes.map((s) => s.id).sort()).toEqual(['identity', 'runtime']);
  });

  it('names the terms carrying no definition, not just how many', () => {
    const out = buildVocabulary([
      row({ ...SERVICE }),
      row({
        concept: 'urn:chorus:domains:vocabulary#bare',
        scheme: 'urn:chorus:domains:vocabulary#identity',
        pref: 'bare',
      }),
    ]);
    expect(out.withoutDefinition).toEqual(['bare']);
  });

  // NEGATIVE PROOF for the line above: with every term defined the list is
  // empty, so a non-empty list means a real gap and not a fold that always
  // reports something.
  it('reports no gap when every term carries a definition', () => {
    const out = buildVocabulary([row({ ...SERVICE })]);
    expect(out.withoutDefinition).toEqual([]);
    expect(out.termCount).toBe(1);
  });

  it('drops a row with no scheme rather than inventing one', () => {
    const out = buildVocabulary([
      row({ concept: 'urn:chorus:domains:vocabulary#loose', pref: 'loose' }),
    ]);
    expect(out.termCount).toBe(0);
    expect(out.schemes).toEqual([]);
  });
});

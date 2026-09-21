/* #4254 — the controlled vocabulary, served from the store.
 *
 * One SPARQL query against urn:chorus:domains:vocabulary returns every
 * (scheme, concept, prefLabel, altLabel, definition, exactMatch) row;
 * buildVocabulary is the pure fold from those rows to the view the page
 * renders. No term is listed in this file — a concept authored into the TTL
 * and deployed appears on the next load.
 *
 * The payload carries `withoutDefinition` because the card asks for the answer
 * to "which terms have no definition" to be REPORTED, not assumed. It is a
 * list, not only a count, so the gap is workable rather than a number.
 */
import type { Request, Response } from 'express';

export interface Term {
  id: string;
  scheme: string;
  prefLabel: string;
  altLabels: string[];
  definition?: string;
  /** The class or property this word names, when it names one. */
  exactMatch?: string;
  note?: string;
}
export interface Scheme { id: string; prefLabel: string; description?: string; terms: Term[] }
export interface Vocabulary {
  schemes: Scheme[];
  termCount: number;
  /** prefLabels of the terms carrying no skos:definition. */
  withoutDefinition: string[];
}

export interface VocabBinding {
  concept?: { value: string };
  scheme?: { value: string };
  schemeLabel?: { value: string };
  schemeDesc?: { value: string };
  pref?: { value: string };
  alt?: { value: string };
  def?: { value: string };
  match?: { value: string };
  note?: { value: string };
}

const local = (v: string | undefined): string => String(v || '').split(/[#/]/).pop() || '';

export function buildVocabulary(rows: VocabBinding[]): Vocabulary {
  const schemes = new Map<string, Scheme>();
  const terms = new Map<string, Term>();

  for (const row of rows) {
    const conceptIri = row.concept?.value;
    const schemeIri = row.scheme?.value;
    if (!conceptIri || !schemeIri) continue;

    const schemeId = local(schemeIri);
    const scheme = schemes.get(schemeId) ?? {
      id: schemeId,
      prefLabel: row.schemeLabel?.value || schemeId,
      ...(row.schemeDesc?.value ? { description: row.schemeDesc.value } : {}),
      terms: [],
    };
    if (!scheme.description && row.schemeDesc?.value) scheme.description = row.schemeDesc.value;
    schemes.set(schemeId, scheme);

    const id = local(conceptIri);
    const key = `${schemeId}/${id}`;
    const term = terms.get(key) ?? {
      id,
      scheme: schemeId,
      prefLabel: row.pref?.value || id,
      altLabels: [],
    };
    if (row.def?.value && !term.definition) term.definition = row.def.value;
    if (row.match?.value && !term.exactMatch) term.exactMatch = local(row.match.value);
    if (row.note?.value && !term.note) term.note = row.note.value;
    const alt = row.alt?.value;
    if (alt && !term.altLabels.includes(alt)) term.altLabels.push(alt);
    if (!terms.has(key)) scheme.terms.push(term);
    terms.set(key, term);
  }

  for (const s of schemes.values()) {
    s.terms.sort((a, b) => a.prefLabel.localeCompare(b.prefLabel));
    for (const t of s.terms) t.altLabels.sort((a, b) => a.localeCompare(b));
  }

  const all = [...terms.values()];
  return {
    schemes: [...schemes.values()].sort((a, b) => a.prefLabel.localeCompare(b.prefLabel)),
    termCount: all.length,
    withoutDefinition: all.filter((t) => !t.definition).map((t) => t.prefLabel).sort(),
  };
}

const VOCAB_QUERY = `PREFIX skos: <http://www.w3.org/2004/02/skos/core#>
PREFIX dcterms: <http://purl.org/dc/terms/>
SELECT ?concept ?scheme ?schemeLabel ?schemeDesc ?pref ?alt ?def ?match ?note WHERE {
  GRAPH <urn:chorus:domains:vocabulary> {
    ?concept a skos:Concept ; skos:inScheme ?scheme ; skos:prefLabel ?pref .
    OPTIONAL { ?scheme skos:prefLabel ?schemeLabel }
    OPTIONAL { ?scheme dcterms:description ?schemeDesc }
    OPTIONAL { ?concept skos:altLabel ?alt }
    OPTIONAL { ?concept skos:definition ?def }
    OPTIONAL { ?concept skos:exactMatch ?match }
    OPTIONAL { ?concept dcterms:description ?note }
  }
}`;

export function vocabularyHandler() {
  return async (_req: Request, res: Response): Promise<void> => {
    const endpoint = (process.env.CHORUS_FUSEKI || 'http://localhost:3030/pods') + '/query';
    try {
      const r = await fetch(endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/x-www-form-urlencoded',
          Accept: 'application/sparql-results+json',
        },
        body: 'query=' + encodeURIComponent(VOCAB_QUERY),
      });
      if (!r.ok) {
        // storeReachable is explicit so an empty vocabulary can never read as
        // "there are no terms" when the truth is "we could not ask".
        res.status(502).json({ error: 'store-unreachable', storeReachable: false, http: r.status });
        return;
      }
      const body = (await r.json()) as { results?: { bindings?: VocabBinding[] } };
      res.json({
        storeReachable: true,
        graph: 'urn:chorus:domains:vocabulary',
        ...buildVocabulary(body.results?.bindings ?? []),
      });
    } catch {
      res.status(502).json({ error: 'store-unreachable', storeReachable: false });
    }
  };
}

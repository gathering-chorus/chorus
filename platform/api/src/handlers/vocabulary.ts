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
  /** The full IRI behind exactMatch — the namespace decides whether a missing
   *  definition is our gap or someone else's vocabulary. Not rendered. */
  exactMatchIri?: string;
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

/** Is this a term the model is expected to define? A concept with no
 *  exactMatch at all is ours by default — it is one of the hand-authored
 *  ones. */
const isOurs = (iri: string | undefined): boolean =>
  iri === undefined || iri.startsWith('https://jeffbridwell.com/');

/** Upsert the scheme this row belongs to. Split out of buildVocabulary to keep
 *  it under the complexity ratchet — the fold does three jobs and each one is
 *  easier to read alone. */
function upsertScheme(schemes: Map<string, Scheme>, row: VocabBinding, schemeId: string): Scheme {
  const scheme = schemes.get(schemeId) ?? {
    id: schemeId,
    prefLabel: row.schemeLabel?.value || schemeId,
    terms: [],
  };
  if (!scheme.description && row.schemeDesc?.value) scheme.description = row.schemeDesc.value;
  schemes.set(schemeId, scheme);
  return scheme;
}

/** A concept arrives once per OPTIONAL combination, so every row repeats these
 *  fields and only the first one carrying each should win. Split out of
 *  upsertTerm to keep both under the complexity ratchet. */
function takeFirstValues(term: Term, row: VocabBinding): void {
  const def = row.def?.value;
  if (def && !term.definition) term.definition = def;

  const match = row.match?.value;
  if (match && !term.exactMatch) {
    term.exactMatch = local(match);
    term.exactMatchIri = match;
  }

  const note = row.note?.value;
  if (note && !term.note) term.note = note;

  const alt = row.alt?.value;
  if (alt && !term.altLabels.includes(alt)) term.altLabels.push(alt);
}

/** Merge one row into its term. */
function upsertTerm(terms: Map<string, Term>, scheme: Scheme, row: VocabBinding, id: string): void {
  const key = `${scheme.id}/${id}`;
  const existing = terms.get(key);
  const term: Term = existing ?? {
    id,
    scheme: scheme.id,
    prefLabel: row.pref?.value || id,
    altLabels: [],
  };

  takeFirstValues(term, row);

  if (!existing) {
    scheme.terms.push(term);
    terms.set(key, term);
  }
}

export function buildVocabulary(rows: VocabBinding[]): Vocabulary {
  const schemes = new Map<string, Scheme>();
  const terms = new Map<string, Term>();

  for (const row of rows) {
    // A concept with no scheme cannot be scoped, so it is dropped rather than
    // pooled with everything else — pooling would invent collisions.
    if (!row.concept?.value || !row.scheme?.value) continue;
    const scheme = upsertScheme(schemes, row, local(row.scheme.value));
    upsertTerm(terms, scheme, row, local(row.concept.value));
  }

  for (const s of schemes.values()) {
    s.terms.sort((a, b) => a.prefLabel.localeCompare(b.prefLabel));
    for (const t of s.terms) t.altLabels.sort((a, b) => a.localeCompare(b));
  }

  const all = [...terms.values()];
  return {
    schemes: [...schemes.values()].sort((a, b) => a.prefLabel.localeCompare(b.prefLabel)),
    termCount: all.length,
    // NAMED, not counted: "which terms have no definition" is a worklist, and
    // a number alone cannot be worked.
    //
    // rdfs:label and rdfs:comment are somebody else's vocabulary — we are never
    // going to define them, so leaving them in puts permanent entries in a list
    // whose whole purpose is that it can reach zero. Same exclusion the
    // generator applies to its ungrounded list; one rule in two places is how
    // two numbers drift apart.
    withoutDefinition: all
      .filter((t) => !t.definition && isOurs(t.exactMatchIri))
      .map((t) => t.prefLabel)
      .sort(),
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

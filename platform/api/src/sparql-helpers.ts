// SPARQL helper functions (extracted from server.ts for #2205 wave 7).
// Pure string transformations — zero external state.

/** Escape a string for safe interpolation into a SPARQL literal. */
export function escSparql(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '');
}

/** Turn a domain or field name into a URI-safe slug. */
export function icdSlug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

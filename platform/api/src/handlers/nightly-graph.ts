// #4156 → #4579 reopen — the nightly runs no longer come from the graph: /nightly
// reads the run log (nightly-log.ts) and the graph stops carrying TestSuiteRun
// (Jeff 10-18 19:10). What stays here is the CSV reading other graph pages use.
import { csvCells } from './nightly-report';

/** A SPARQL read that answers CSV text, or null when the store did not answer. */
export type CsvQuery = (sparql: string) => Promise<string | null>;

/** CSV text to one record per row, keyed by the header. */
export function csvRecords(csv: string): Record<string, string>[] {
  const lines = csv.split('\n').map((l) => l.replace(/\r$/, '')).filter((l) => l !== '');
  if (lines.length === 0) return [];
  const head = csvCells(lines[0]);
  return lines.slice(1).map((l) => {
    const cells = csvCells(l);
    return Object.fromEntries(head.map((h, i) => [h, cells.slice(i, i + 1).join('')]));
  });
}

/** The CsvQuery the routes use: Fuseki's query endpoint, CSV out. */
export function fusekiCsv(endpoint: string, fetchFn: typeof fetch = fetch): CsvQuery {
  return async (sparql: string) => {
    try {
      const r = await fetchFn(`${endpoint}?query=${encodeURIComponent(sparql)}`, {
        headers: { Accept: 'text/csv' }, signal: AbortSignal.timeout(20000) });
      return r.ok ? await r.text() : null;
    } catch { return null; }
  };
}

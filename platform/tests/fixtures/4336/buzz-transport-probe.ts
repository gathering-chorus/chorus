// #4336 — runs the REAL Buzz relay fetch (index-all-sources-deps.ts fetchBuzz)
// against a stubbed Fuseki (globalThis.fetch) and whatever `ssh` is first on
// PATH (the bats suite puts a recorder there). Prints the row count on success.
// Usage: tsx buzz-transport-probe.ts <path-to-index-all-sources-deps.ts>
const target = process.argv[2];
(globalThis as { fetch: unknown }).fetch = async () =>
  new Response(JSON.stringify({ results: { bindings: [] } }), {
    status: 200,
    headers: { 'Content-Type': 'application/sparql-results+json' },
  });
(async () => {
  const mod = await import(target);
  const deps = mod.buildIndexAllSourcesDeps({ dbPath: ':memory:', repoRoot: process.cwd() });
  if (typeof deps.fetchBuzz !== 'function') {
    console.log('NO-FETCHBUZZ');
    process.exit(3);
  }
  const rows = await deps.fetchBuzz();
  console.log(`rows=${rows.length}`);
})().catch((e) => {
  console.error(`probe error: ${e instanceof Error ? e.message : String(e)}`);
  process.exit(1);
});

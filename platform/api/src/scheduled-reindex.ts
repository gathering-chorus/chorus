// Scheduled reindex (extracted from server.ts for #2205 wave 15).
// Keeps index_freshness sources current (#1960). Serializes overlapping
// invocations so the 15-min timer can fire without stomping an in-flight run.

export interface ScheduledReindexDeps {
  indexAllSources: () => Promise<Record<string, any>>;
  log?: (m: string) => void;
  error?: (m: string) => void;
}

export function createScheduledReindex(deps: ScheduledReindexDeps): () => Promise<void> {
  const log = deps.log ?? ((m) => console.log(m));
  const error = deps.error ?? ((m) => console.error(m));
  let running = false;
  return async (): Promise<void> => {
    if (running) return;
    running = true;
    try {
      const result = await deps.indexAllSources();
      const total = Object.values(result)
        .filter(v => typeof v === 'string' && v.startsWith('indexed '))
        .length;
      log(`[reindex] scheduled run complete — ${total} sources indexed`);
    } catch (err: any) {
      error(`[reindex] scheduled run failed: ${err.message}`);
    } finally {
      running = false;
    }
  };
}

// @domain: properties
// #4889 — types for the one TypeScript global-property reader (global.js).
/** where: 'row' = the graph has no usable row; 'door' = the door did not give a readable answer. */
export type GlobalRead = { ok: true; value: string } | { ok: false; where: 'row' | 'door'; error: string };
export declare function doorUrl(): string;
export declare function globalValueFromBody(body: unknown, key: string): GlobalRead;
export declare function readGlobal(
  key: string,
  opts?: { door?: string; fetchFn?: typeof fetch; timeoutMs?: number },
): Promise<GlobalRead>;

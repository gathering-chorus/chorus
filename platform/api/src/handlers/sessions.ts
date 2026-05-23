/**
 * Session replay handlers (#2173 AC4).
 *
 * Three related GET endpoints under /api/chorus/sessions extracted from
 * server.ts:124-145. The real logic already lives in src/session-replay.ts;
 * this module just wraps those calls in the uniform `{status, body}` shape
 * and exposes them for unit testing with injected deps.
 *
 * `FetchResult` gets an optional `contentType` for the one handler that
 * returns plain text (the action log). Default 'application/json' when
 * omitted — so every previous extraction continues to work unchanged.
 */

export interface FetchResult {
  status: number;
  body: unknown;
  contentType?: string;
}

// #3039 — session-replay reads are now async (rrweb files can be MB, off the
// loop). Deps accept Promise OR sync value so existing tests' sync mocks still
// typecheck; the handlers await either way.
export interface SessionsDeps {
  listSessions: () => Promise<unknown> | unknown;
  getSession: (id: string) => Promise<unknown | null | undefined> | unknown | null | undefined;
  getSessionLog: (id: string) => Promise<string | null> | string | null;
  isValidSessionId: (id: string) => boolean;
}

export async function fetchSessionList(deps: SessionsDeps): Promise<FetchResult> {
  try {
    return { status: 200, body: await deps.listSessions() };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { status: 500, body: { error: message } };
  }
}

export async function fetchSessionById(deps: SessionsDeps, idInput: unknown): Promise<FetchResult> {
  const id = typeof idInput === 'string' ? idInput : '';
  if (!deps.isValidSessionId(id)) {
    return { status: 400, body: { error: 'invalid session id' } };
  }
  const session = await deps.getSession(id);
  if (!session) {
    return { status: 404, body: { error: 'session not found' } };
  }
  return { status: 200, body: session };
}

export async function fetchSessionLog(deps: SessionsDeps, idInput: unknown): Promise<FetchResult> {
  const id = typeof idInput === 'string' ? idInput : '';
  if (!deps.isValidSessionId(id)) {
    return { status: 400, body: { error: 'invalid session id' } };
  }
  const log = await deps.getSessionLog(id);
  if (log === null) {
    return { status: 404, body: { error: 'log not found' } };
  }
  return { status: 200, body: log, contentType: 'text/plain' };
}

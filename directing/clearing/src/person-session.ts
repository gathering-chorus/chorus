/**
 * #4412 — a person's browser Session, written by the Clearing.
 *
 * Jeff's sign-in at the Clearing is what lets a role log in (chorus-principal
 * refuses without an open browser Session for a person). So the Clearing writes
 * that row at sign-in, keeps lastSeenAt current while he uses it, and closes it
 * at sign-out. The row names every role Session that was open at sign-in in
 * `binds`; closing the row ends the binding. Role rows are never written here.
 *
 * The row is written AS the person, with his own sign-in token (audience = the
 * Clearing's client id, accepted by chorus-oidc since #4412). No service key.
 * The token is refreshed with his refresh token when it expires, and the whole
 * record lives in one 0600 file so a Clearing restart can still close the row.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';

export interface SignInClaims {
  webid: string;
  idToken: string;
  refreshToken?: string;
  iat: number; // seconds
  exp: number; // seconds
}

export interface PersonSessionRecord {
  key: string;
  principal: string; // bare name, e.g. "jeff"
  rowName: string; // the name the API stored
  row: Record<string, unknown>;
  idToken: string;
  refreshToken?: string;
  exp: number; // seconds
  lastSeenWrite: number; // ms
}

export interface PersonSessionDeps {
  api: string; // athena-make base, e.g. http://localhost:3360
  fetchImpl: typeof fetch;
  now: () => number; // ms
  storePath: string;
  /** Exchange a refresh token for fresh tokens; null when it cannot. */
  refresh: (refreshToken: string) => Promise<SignInClaims | null>;
  log?: (line: string) => void;
}

/**
 * How long a sign-in lasts: the session cookie's life. The row's expiresAt is
 * this, not the token's exp — chorus-principal reads expiresAt to decide he is
 * signed in, and his token expires within the hour while he is still here.
 */
export const SIGN_IN_LASTS_MS = 30 * 24 * 60 * 60 * 1000;

/** How often lastSeenAt may be written while he is active. */
export const SEEN_EVERY_MS = 60_000;

const iso = (ms: number): string => new Date(ms).toISOString().replace(/\.\d{3}Z$/, 'Z');

/** Lowercase, one dash, none at the ends: the way the API stores a name. */
export function slug(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
}

/** The row for a person's browser Session. Pure: everything it needs is passed in. */
export function personSessionRow(
  principal: string,
  claims: SignInClaims,
  openRoleSessions: string[],
  nowMs: number,
  name: string,
): Record<string, unknown> {
  const row: Record<string, unknown> = {
    name,
    label: `${principal} signed in to the Clearing ${iso(nowMs)}`,
    ownedBy: principal,
    actsAs: principal,
    channel: 'browser',
    tokenId: tokenIdOf(claims.idToken),
    issuedAt: iso(claims.iat * 1000),
    expiresAt: iso(nowMs + SIGN_IN_LASTS_MS),
    startedAt: iso(nowMs),
    lastSeenAt: iso(nowMs),
    sessionState: 'open',
  };
  if (openRoleSessions.length) row.binds = openRoleSessions;
  return row;
}

/** The closed form of the row: same row, state closed, endedAt now. */
export function closedRow(row: Record<string, unknown>, nowMs: number): Record<string, unknown> {
  return { ...row, sessionState: 'closed', endedAt: iso(nowMs) };
}

/** The token's own id (jti), else its hash: never the token itself. */
export function tokenIdOf(jwt: string): string {
  try {
    const payload = JSON.parse(Buffer.from(jwt.split('.')[1] ?? '', 'base64url').toString());
    if (typeof payload.jti === 'string' && payload.jti) return payload.jti;
  } catch { /* fall through to the hash */ }
  return crypto.createHash('sha256').update(jwt).digest('hex').slice(0, 32);
}

/** The names of the role Sessions that are open now (never a person's). */
export function openRoleSessionNames(list: unknown): string[] {
  const items = pickItems(list);
  return items
    .filter((r) => r.sessionState === 'open' && typeof r.actsAs === 'string' && r.actsAs.startsWith('role-'))
    .map((r) => String(r.name))
    .filter(Boolean);
}

function pickItems(list: unknown): Array<Record<string, unknown>> {
  if (Array.isArray(list)) return list as Array<Record<string, unknown>>;
  const o = (list ?? {}) as { items?: unknown; data?: unknown; rows?: unknown };
  for (const v of [o.items, o.data, o.rows]) {
    if (Array.isArray(v)) return v as Array<Record<string, unknown>>;
  }
  return [];
}

// --- the store: one 0600 file, keyed by the cookie's session key ------------

// The path is the Clearing's own config (CLEARING_PERSON_SESSIONS), never
// request input; hence the fs-filename disables below.
function readStore(p: string): Map<string, PersonSessionRecord> {
  try {
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- config path, not input
    const o = JSON.parse(fs.readFileSync(p, 'utf-8')) as Record<string, PersonSessionRecord>;
    return new Map(Object.entries(o));
  } catch {
    return new Map();
  }
}

function writeStore(p: string, s: Map<string, PersonSessionRecord>): void {
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- config path, not input
  fs.mkdirSync(path.dirname(p), { recursive: true });
  const tmp = `${p}.${process.pid}.tmp`;
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- config path, not input
  fs.writeFileSync(tmp, JSON.stringify(Object.fromEntries(s)), { mode: 0o600 });
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- config path, not input
  fs.renameSync(tmp, p);
}

export function getRecord(deps: PersonSessionDeps, key: string): PersonSessionRecord | null {
  return readStore(deps.storePath).get(key) ?? null;
}

// --- the API calls ----------------------------------------------------------

async function send(
  deps: PersonSessionDeps,
  method: 'POST' | 'PUT',
  url: string,
  token: string,
  body: unknown,
): Promise<{ status: number; body: Record<string, unknown> }> {
  const res = await deps.fetchImpl(url, {
    method,
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
    body: JSON.stringify(body),
  });
  let parsed: Record<string, unknown> = {};
  try { parsed = (await res.json()) as Record<string, unknown>; } catch { /* no body */ }
  return { status: res.status, body: parsed };
}

/** A token that is still good for a minute, refreshing when it is not. */
async function liveToken(deps: PersonSessionDeps, rec: PersonSessionRecord): Promise<string | null> {
  if (rec.exp * 1000 - deps.now() > 60_000) return rec.idToken;
  if (!rec.refreshToken) return null;
  const fresh = await deps.refresh(rec.refreshToken);
  if (!fresh) return null;
  rec.idToken = fresh.idToken;
  rec.exp = fresh.exp;
  if (fresh.refreshToken) rec.refreshToken = fresh.refreshToken;
  return rec.idToken;
}

function storedName(reply: Record<string, unknown>, sent: string): string {
  const data = (reply.data ?? {}) as Record<string, unknown>;
  const iri = typeof data.iri === 'string' ? data.iri : '';
  const tail = iri.split('#').pop() ?? '';
  const name = tail.replace(/^session-/, '');
  return name || sent;
}

async function listOpenRoleSessions(deps: PersonSessionDeps): Promise<string[] | null> {
  try {
    const r = await deps.fetchImpl(`${deps.api}/v1/identity/sessions?limit=5000`);
    return r.ok ? openRoleSessionNames(await r.json()) : null;
  } catch { return null; }
}

/**
 * Sign-in: write his open browser Session, naming the role Sessions open now.
 * Returns the store key for the cookie, or null (with the reason logged) when
 * the row could not be written. A failed write never blocks the sign-in.
 */
export async function openPersonSession(
  deps: PersonSessionDeps,
  principal: string,
  claims: SignInClaims,
): Promise<string | null> {
  const log = deps.log ?? (() => {});
  // no list: the row goes out with no binds, and the log line says 0
  const roles = (await listOpenRoleSessions(deps)) ?? [];
  const nowMs = deps.now();
  const name = slug(`${principal}-browser-${nowMs.toString(36)}`);
  const row = personSessionRow(principal, claims, roles, nowMs, name);
  let reply;
  try {
    reply = await send(deps, 'POST', `${deps.api}/v1/identity/sessions`, claims.idToken, row);
  } catch (e) {
    log(`person-session: ${principal} sign-in row NOT written: ${(e as Error).message}`);
    return null;
  }
  if (reply.status !== 200 && reply.status !== 201) {
    log(`person-session: ${principal} sign-in row NOT written: HTTP ${reply.status} ${JSON.stringify(reply.body).slice(0, 200)}`);
    return null;
  }
  const rowName = storedName(reply.body, name);
  const key = crypto.randomBytes(18).toString('base64url');
  const store = readStore(deps.storePath);
  store.set(key, {
    key, principal, rowName, row: { ...row, name: rowName },
    idToken: claims.idToken, refreshToken: claims.refreshToken, exp: claims.exp, lastSeenWrite: nowMs,
  });
  writeStore(deps.storePath, store);
  log(`person-session: ${principal} signed in, session ${rowName}, binds ${roles.length} role session(s)`);
  return key;
}

/** Activity: keep lastSeenAt current, at most once a minute. */
export async function touchPersonSession(deps: PersonSessionDeps, key: string): Promise<boolean> {
  const store = readStore(deps.storePath);
  const rec = store.get(key);
  if (!rec) return false;
  const nowMs = deps.now();
  if (nowMs - rec.lastSeenWrite < SEEN_EVERY_MS) return true;
  const token = await liveToken(deps, rec);
  if (!token) return false;
  // a role logged in while he is signed in is bound too (Jeff: "that generates
  // a binding to me to each of u"); a role logged out drops off
  const roles = await listOpenRoleSessions(deps);
  const row: Record<string, unknown> = { ...rec.row, lastSeenAt: iso(nowMs) };
  if (roles) { if (roles.length) row.binds = roles; else delete row.binds; }
  const reply = await send(deps, 'PUT', `${deps.api}/v1/identity/sessions/${rec.rowName}`, token, row);
  if (reply.status >= 300) return false;
  store.set(key, { ...rec, row, lastSeenWrite: nowMs });
  writeStore(deps.storePath, store);
  return true;
}

/** Sign-out (or the cookie seen expired): close his row. Its binds end with it. */
export async function closePersonSession(deps: PersonSessionDeps, key: string): Promise<boolean> {
  const log = deps.log ?? (() => {});
  const store = readStore(deps.storePath);
  const rec = store.get(key);
  if (!rec) return false;
  const token = await liveToken(deps, rec);
  let ok = false;
  if (token) {
    const reply = await send(deps, 'PUT', `${deps.api}/v1/identity/sessions/${rec.rowName}`, token, closedRow(rec.row, deps.now()));
    ok = reply.status < 300;
    if (!ok) log(`person-session: ${rec.principal} sign-out row NOT closed: HTTP ${reply.status}`);
  } else {
    log(`person-session: ${rec.principal} sign-out row NOT closed: no live token`);
  }
  // Forget the tokens either way: a sign-out must not leave them on disk.
  store.delete(key);
  writeStore(deps.storePath, store);
  return ok;
}

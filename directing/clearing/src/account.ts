/**
 * #3679 — self-service password change, called SERVER-SIDE against localhost CSS
 * with the Host-override so the CSS account/admin API NEVER touches the public
 * tunnel (preserves the #3669 fork-B boundary: /.account/account/* is localhost-only).
 *
 * Security model (Silas's spec):
 *  - The account changed is BOUND TO THE SESSION WebID. After authenticating as the
 *    user (their own email + current password), we read that account's own linked
 *    WebIDs and REFUSE unless the caller's session WebID is one of them. So a client
 *    can only change the password of the identity they're signed in as — naming
 *    someone else's email fails the ownership check (and needs their password anyway).
 *  - No admin credential lives here: every CSS call uses the USER's own account token,
 *    obtained with the current password they supplied.
 *  - Passwords are NEVER logged or echoed; only structured reasons cross a boundary,
 *    and error copy never contains a secret.
 */

export type ChangeReason = 'weak-password' | 'bad-credentials' | 'not-your-account' | 'css-error';
export type ChangeResult = { ok: true } | { ok: false; reason: ChangeReason; message: string };

export interface ChangeParams {
  sessionWebid: string; // from the signed session cookie — the identity we're bound to
  email: string;        // the login to authenticate with (client input, VALIDATED against session)
  oldPassword: string;
  newPassword: string;
}

const CSS_HOST = 'id.lightlifeurbangardens.com';

/** Host-override headers: reach localhost CSS while it thinks it's serving the public
 *  origin — the same trick seed-css.sh + the #3669 cutover used. */
function h(token?: string): Record<string, string> {
  const base: Record<string, string> = {
    Host: CSS_HOST,
    'X-Forwarded-Proto': 'https',
    'X-Forwarded-Host': CSS_HOST,
    'Content-Type': 'application/json',
  };
  if (token) base.Authorization = `CSS-Account-Token ${token}`;
  return base;
}

/** Compare WebIDs ignoring the trailing `#me` fragment and any trailing slash. */
export function sameWebid(a: string, b: string): boolean {
  const norm = (w: string) => w.replace(/#.*$/, '').replace(/\/$/, '');
  return !!a && !!b && norm(a) === norm(b);
}

/** Rewrite a CSS control URL (public origin) to the local endpoint, preserving path. */
function local(url: string, cssBase: string): string {
  return url.replace(/^https?:\/\/[^/]+/, cssBase);
}

export async function changePassword(
  p: ChangeParams,
  cssBase = 'http://localhost:3001',
  fetchImpl: typeof fetch = fetch,
): Promise<ChangeResult> {
  if (!p.sessionWebid) return { ok: false, reason: 'not-your-account', message: 'Please sign in again.' };
  if (!p.email || !p.oldPassword) return { ok: false, reason: 'bad-credentials', message: 'Enter your email and current password.' };
  if (p.newPassword.length < 8) return { ok: false, reason: 'weak-password', message: 'New password must be at least 8 characters.' };

  // 1. authenticate as the user with their CURRENT password
  const loginRes = await fetchImpl(`${cssBase}/.account/login/password/`, {
    method: 'POST', headers: h(), body: JSON.stringify({ email: p.email, password: p.oldPassword }),
  });
  if (!loginRes.ok) return { ok: false, reason: 'bad-credentials', message: 'That email or current password is incorrect.' };
  const token = ((await loginRes.json()) as { authorization?: string }).authorization;
  if (!token) return { ok: false, reason: 'bad-credentials', message: 'That email or current password is incorrect.' };

  // fetch the authed account controls (account-scoped endpoints)
  const controls = ((await (await fetchImpl(`${cssBase}/.account/`, { headers: h(token) })).json()) as {
    controls?: { account?: { webId?: string }; password?: { create?: string } };
  }).controls ?? {};
  const webidCtrl = controls.account?.webId;
  const pwCtrl = controls.password?.create;
  if (!webidCtrl || !pwCtrl) return { ok: false, reason: 'css-error', message: 'The identity server didn’t return the expected account controls.' };

  // 2. BIND TO SESSION — this account must own the session WebID, else refuse
  const links = Object.keys(((await (await fetchImpl(local(webidCtrl, cssBase), { headers: h(token) })).json()) as {
    webIdLinks?: Record<string, string>;
  }).webIdLinks ?? {});
  if (!links.some((l) => sameWebid(l, p.sessionWebid))) {
    return { ok: false, reason: 'not-your-account', message: 'You can only change the password for the identity you’re signed in as.' };
  }

  // 3. locate this email's login and POST the change ({oldPassword,newPassword})
  const logins = ((await (await fetchImpl(local(pwCtrl, cssBase), { headers: h(token) })).json()) as {
    passwordLogins?: Record<string, string>;
  }).passwordLogins ?? {};
  const loginUrl = logins[p.email];
  if (!loginUrl) return { ok: false, reason: 'css-error', message: 'Couldn’t locate your login record.' };
  const chRes = await fetchImpl(local(loginUrl, cssBase), {
    method: 'POST', headers: h(token), body: JSON.stringify({ oldPassword: p.oldPassword, newPassword: p.newPassword }),
  });
  if (!chRes.ok) return { ok: false, reason: 'css-error', message: 'The identity server refused the change — check your current password and try again.' };
  return { ok: true };
}

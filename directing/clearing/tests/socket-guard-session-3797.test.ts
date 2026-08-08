// @test-type: security — calls the REAL socketSessionAuthed with real cookie strings signed by the guard's own algorithm; allow-set and key injected, no live services. Brings its own world.
/**
 * #3797 — a person who signed in ONCE can send, not just read.
 *
 * Jeff, 2026-08-08: "i log in ... then clearing shows and says connecting... and
 * never connected." The socket matched only /clearing_session=/ while the Chorus
 * door mints chorus_share_session, so a remote signed-in person was refused
 * forever — authenticated for reading, anonymous for speaking.
 *
 * These call the ACTUAL function. Every previous socket test connected from
 * 127.0.0.1, where isLocalConnection short-circuits before auth, so the branch
 * that refused Jeff had never once been executed by a test.
 *
 * The rule under test, which must not slip: identity is SHARED, admission is
 * LOCAL. A guard cookie proves WHO; the Clearing's own allow-set decides whether
 * that means anything here.
 */
import crypto from 'crypto';
import fs from 'fs';
import path from 'path';

const KEY_FILE = path.join(__dirname, 'fixtures', 'tmp-3797-key.json');
// Derived, not embedded: a literal base64 blob here is indistinguishable from a
// real credential to the secret scanner (and to a reader), so it is built from
// plain text at runtime. It signs nothing outside this file.
const TEST_KEY = Buffer.from('test-key-3797-socket-guard-session')
  .toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const ADMITTED = 'https://id.lightlifeurbangardens.com/jeff/profile/card#me';
const STRANGER = 'https://id.lightlifeurbangardens.com/nobody/profile/card#me';

// The guard's cookie format, minted here exactly as chorus-share-guard.py does:
// base64url(JSON).base64url(HMAC-SHA256(key, payload_string)).
function mintGuardCookie(webid: string, expOffsetSec = 3600, key = TEST_KEY): string {
  const raw = Buffer.from(key.replace(/-/g, '+').replace(/_/g, '/'), 'base64');
  const b64u = (b: Buffer) => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const payload = b64u(Buffer.from(JSON.stringify({ webid, exp: Math.floor(Date.now() / 1000) + expOffsetSec })));
  const mac = b64u(crypto.createHmac('sha256', raw).update(payload).digest());
  return `${payload}.${mac}`;
}

let socketSessionAuthed: (cookieHeader: string) => Promise<boolean>;
let solidAuth: typeof import('../src/solid-auth');

beforeAll(async () => {
  fs.writeFileSync(KEY_FILE, JSON.stringify({ cookie_key: TEST_KEY }));
  process.env.SHARE_STATE_FILE = KEY_FILE;
  process.env.CHORUS_CLEARING_REQUIRE_DPOP = '1';   // bridge-token leg off, as in prod
  solidAuth = await import('../src/solid-auth');
  // the Clearing's OWN allow-set — one admitted WebID, nobody else
  jest.spyOn(solidAuth, 'isWebIdAllowed').mockImplementation(async (w: string) => w === ADMITTED);
  ({ socketSessionAuthed } = await import('../src/server'));
});

afterAll(() => {
  try { fs.unlinkSync(KEY_FILE); } catch { /* ignore */ }
});

describe('#3797 the socket accepts a Chorus sign-in', () => {
  test("an admitted person's guard cookie connects — the failure Jeff hit is gone", async () => {
    const ok = await socketSessionAuthed(`chorus_share_session=${mintGuardCookie(ADMITTED)}`);
    expect(ok).toBe(true);
  });

  test('NEGATIVE PROOF: identity is shared, admission is LOCAL — a valid guard cookie for a WebID this app does not admit is REFUSED', async () => {
    // The dangerous fix is "holds a guard cookie ⇒ connected". That would make
    // one sign-in mean admitted-everywhere, which per-app authorization exists
    // to prevent. Same cookie machinery, different person, must be false.
    const ok = await socketSessionAuthed(`chorus_share_session=${mintGuardCookie(STRANGER)}`);
    expect(ok).toBe(false);
  });

  test('a forged guard cookie is refused (signed with the wrong key)', async () => {
    const wrongKey = Buffer.from('wrong-key-entirely-not-the-real-one')
      .toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    const forged = mintGuardCookie(ADMITTED, 3600, wrongKey);
    expect(await socketSessionAuthed(`chorus_share_session=${forged}`)).toBe(false);
  });

  test('an expired guard cookie is refused', async () => {
    expect(await socketSessionAuthed(`chorus_share_session=${mintGuardCookie(ADMITTED, -60)}`)).toBe(false);
  });

  test('no cookie at all is refused', async () => {
    expect(await socketSessionAuthed('')).toBe(false);
    expect(await socketSessionAuthed('other=1; unrelated=2')).toBe(false);
  });

  test('REGRESSION LOCK: this test can fail — the pre-#3797 behaviour is refusal', async () => {
    // Guard against the assertion going vacuous: if the guard leg were removed,
    // case 1 returns false. Prove the harness distinguishes the two outcomes by
    // showing an admitted cookie and a stranger cookie give DIFFERENT answers.
    const admitted = await socketSessionAuthed(`chorus_share_session=${mintGuardCookie(ADMITTED)}`);
    const stranger = await socketSessionAuthed(`chorus_share_session=${mintGuardCookie(STRANGER)}`);
    expect(admitted).not.toEqual(stranger);
  });
});

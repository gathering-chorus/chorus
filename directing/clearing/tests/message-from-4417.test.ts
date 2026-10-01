// @test-type: unit:security — POST /api/message to the in-process Clearing, the security graph answered by a stub; who a post is "from".
/**
 * #4417 (Silas 08-25, 10-01) — `from` was whatever the body said, so the shared
 * machine credential could post as Jeff and the room and team memory recorded it
 * as his words. Now a signed-in person posts as their own principal, and the
 * machine credential may use any name except a person's.
 */
import { useInProcessClearing, ALLOWED } from './lib/in-process-clearing'; // first: sets the temp guard key path
import * as fs from 'fs';
import * as path from 'path';

const gate = useInProcessClearing();
const token = () => fs.readFileSync(path.join(process.env.CLEARING_CHORUS_HOME as string, 'bridge-auth-token'), 'utf8').trim();

async function post(body: Record<string, string>, headers: Record<string, string> = {}): Promise<{ status: number; marker: string }> {
  const res = await fetch(`${gate.base()}/api/message`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token()}`, ...headers },
    body: JSON.stringify(body),
  });
  return { status: res.status, marker: body.text };
}

async function rowFor(marker: string): Promise<{ from: string } | undefined> {
  const list = await (await fetch(`${gate.base()}/api/messages?includeHidden=1&limit=500`)).json() as Array<{ from: string; text: string }>;
  return list.find((m) => m.text.includes(marker));
}

describe('#4417 who a message is from', () => {
  test('the machine credential posting as a role: accepted, in the room under that name', async () => {
    const r = await post({ from: 'silas', text: `role-${Date.now()}` });
    expect(r.status).toBe(200);
    expect((await rowFor(r.marker))?.from).toBe('silas');
  });

  test('NEGATIVE PROOF: the machine credential posting as Jeff is refused 403, and nothing is stored', async () => {
    const r = await post({ from: 'Jeff', text: `spoof-${Date.now()}` });
    expect(r.status).toBe(403);
    expect(await rowFor(r.marker)).toBeUndefined();
  });

  test('any person principal is protected, not just Jeff (read from the Principal rows)', async () => {
    const r = await post({ from: 'marknakib', text: `spoof2-${Date.now()}` });
    expect(r.status).toBe(403);
    expect(await rowFor(r.marker)).toBeUndefined();
  });

  // Silas's review of 0532077e2: lowercasing alone let these through.
  test.each([
    ['the principal id', 'principal-jeff'],
    ['the full IRI', 'https://jeffbridwell.com/chorus#principal-jeff'],
    ['the label', 'Jeff Bridwell'],
    ['the label, spaced and cased', '  jeff   BRIDWELL '],
    ['the WebID', 'https://id.example/jeff/profile/card#me'],
    ['the WebID without #me', 'https://id.example/jeff/profile/card'],
    ['the host account', 'jeffbridwell'],
    ['a zero-width space inside', 'je\u200Bff'],
    ['full-width letters', 'Ｊｅｆｆ'],
  ])('NEGATIVE PROOF: the machine credential using %s ("%s") is refused 403', async (_what, name) => {
    const r = await post({ from: name, text: `alias-${Date.now()}-${Math.random()}` });
    expect(r.status).toBe(403);
    expect(await rowFor(r.marker)).toBeUndefined();
  });

  test('a signed-in person posts as themselves, whatever the body claims', async () => {
    const r = await post({ from: 'silas', text: `session-${Date.now()}` }, { cookie: gate.guardCookie(ALLOWED) });
    expect(r.status).toBe(200);
    expect((await rowFor(r.marker))?.from).toBe('jeff');
  });
});

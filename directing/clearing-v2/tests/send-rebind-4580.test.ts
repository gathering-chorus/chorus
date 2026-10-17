// @test-type: unit — drives the page's send with a fake fetch; no server, no network
// @domain: messages
// @card: #4580
// @owner: wren
// #4580 reopen — Jeff 2026-10-17 15:11 "still cant message in clearing v2". The server keeps
// bound room keys in memory, so after any restart this browser's key is unknown and every
// send is "not signed in". The page now binds its key with the session and retries once.

const NOT_SIGNED_IN = { ok: false, error: 'not-signed-in', detail: 'Not signed in here.', signInUrl: 'https://door/' };

function ui() {
  return {
    inputEl: { value: 'hello?' },
    statusEl: { textContent: '', style: { display: 'none' } },
    btnEl: { textContent: 'Send', className: '', classList: { add: jest.fn(), remove: jest.fn() } },
  };
}

const reply = (status: number, body: unknown) => ({ ok: status < 400, status, json: async () => body, text: async () => JSON.stringify(body) });

test('a send refused as not-signed-in binds the key and goes through on the retry', async () => {
  const { handleSendMessage } = require('../public/app.js');
  const calls: string[] = [];
  const fetchFn = jest.fn(async (url: string) => {
    calls.push(url);
    if (url.endsWith('/api/room/bind')) return reply(200, { ok: true });
    return calls.filter((u) => u.endsWith('/api/send')).length === 1 ? reply(401, NOT_SIGNED_IN) : reply(200, { ok: true, id: 'e1', from: 'jeff' });
  });
  const u = ui();
  await handleSendMessage('hello?', { ...u, fetchFn });
  expect(calls).toEqual(['/api/send', '/api/room/bind', '/api/send']);
  expect(u.btnEl.textContent).toBe('Sent');
  expect(u.inputEl.value).toBe('');
});

test('NEGATIVE PROOF: with no session the bind is refused, the send is not retried, and the page says sign in', async () => {
  const { handleSendMessage } = require('../public/app.js');
  const calls: string[] = [];
  const fetchFn = jest.fn(async (url: string) => {
    calls.push(url);
    return reply(401, NOT_SIGNED_IN);
  });
  const u = ui();
  await handleSendMessage('hello?', { ...u, fetchFn });
  expect(calls).toEqual(['/api/send', '/api/room/bind']);
  expect(u.statusEl.textContent).toMatch(/not signed in/i);
  expect(u.inputEl.value).toBe('hello?');
});

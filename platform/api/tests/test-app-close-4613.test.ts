// @test-type: unit — a local http server and one held connection; no app, no network.
// @domain: werk
// @card: #4613
// @owner: silas
/**
 * #4613 — the harness close() cannot hang on a connection the server still
 * holds. Measured: instance-explorer's afterAll close() passed jest's 5 s hook
 * timeout in pipeline runs (#4530 run 5, #4595 cicd 6 and 8, #4595 pair 2) with
 * every test passing; locally it closes in 1.6-4.4 s.
 */
import http, { type Server } from 'http';
import type { AddressInfo } from 'net';
import { closeServer } from './lib/test-app';

/** A server holding one request open (the handler never answers), as a slow route under load does. */
async function serverWithHeldRequest(): Promise<{ server: Server; held: Promise<void> }> {
  const server = http.createServer(() => { /* never responds: the request stays in flight */ });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const { port } = server.address() as AddressInfo;
  let opened!: () => void;
  const open = new Promise<void>((r) => { opened = r; });
  server.on('request', () => opened());
  const held = new Promise<void>((resolve) => {
    const req = http.get({ host: '127.0.0.1', port, agent: new http.Agent({ keepAlive: true }) });
    req.on('error', () => resolve()); // the close ends it; that is the point
  });
  await open;
  return { server, held };
}

const settledWithin = (p: Promise<unknown>, ms: number) =>
  Promise.race([p.then(() => true, () => true), new Promise<boolean>((r) => setTimeout(() => r(false), ms))]);

test('close() finishes under 1 s while a request is still held open', async () => {
  const { server, held } = await serverWithHeldRequest();
  const t0 = Date.now();
  await closeServer(server);
  expect(Date.now() - t0).toBeLessThan(1000);
  await held;
});

// NEGATIVE PROOF: the old close (server.close() alone) is still waiting after
// 1.5 s on the same held request — the hang the pipeline hit at 5 s.
test('negative proof: server.close() alone does not finish while the request is held', async () => {
  const { server, held } = await serverWithHeldRequest();
  const oldClose = new Promise<void>((resolve) => server.close(() => resolve()));
  expect(await settledWithin(oldClose, 1500)).toBe(false);
  server.closeAllConnections(); // clean up: the old close is still pending, end what it waits on
  await oldClose;
  await held;
});

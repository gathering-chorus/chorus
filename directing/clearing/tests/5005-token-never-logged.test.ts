// @test-type: unit — imports the server module with a planted token; no ports, no network.
// @domain: security
// @card: #5005
// @owner: silas
/**
 * #5005 — the Clearing never writes its remote-access token to a log.
 *
 * server.ts printed the bridge token at every start ("remote access token:
 * <value>"), so every boot put the live secret into the service log and from
 * there into Loki, where every role account can read it.
 *
 * The check plants a known token, loads the module fresh the way a start does,
 * and captures everything written to the console AND to stdout/stderr directly.
 * It fails if the value appears anywhere. It also requires the start to print
 * the notice, so a capture that saw nothing cannot pass by silence.
 * NEGATIVE PROOF: with the old console.log line restored, the first test fails.
 */

import fs from 'fs';
import os from 'os';
import path from 'path';
import { bridgeTokenNotice } from '../src/server';

const KNOWN = 'tok5005' + 'a1b2c3d4e5f60718293a4b5c6d7e8f90';

let home: string;
let captured: string[];
const restore: Array<() => void> = [];

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-5005-'));
  fs.writeFileSync(path.join(home, 'bridge-auth-token'), KNOWN + '\n');
  captured = [];
  const grab = (...a: unknown[]) => { captured.push(a.map(String).join(' ')); };
  for (const m of ['log', 'info', 'warn', 'error', 'debug'] as const) {
    const spy = jest.spyOn(console, m).mockImplementation(grab);
    restore.push(() => spy.mockRestore());
  }
  for (const stream of [process.stdout, process.stderr]) {
    const orig = stream.write;
    (stream as any).write = (chunk: unknown) => { captured.push(String(chunk)); return true; };
    restore.push(() => { (stream as any).write = orig; });
  }
});

afterEach(() => {
  while (restore.length) restore.pop()!();
  fs.rmSync(home, { recursive: true, force: true });
});

/** Load server.ts fresh, as a start does, with the planted token home. */
function startWithPlantedToken(): void {
  const prior = process.env.CLEARING_CHORUS_HOME;
  jest.isolateModules(() => {
    process.env.CLEARING_CHORUS_HOME = home;
    try {
      require('../src/server');
    } finally {
      process.env.CLEARING_CHORUS_HOME = prior;
    }
  });
}

describe('#5005 the bridge token is never logged', () => {
  test('a start with a known token never writes its value, and prints the notice', () => {
    startWithPlantedToken();
    const all = captured.join('\n');
    expect(bridgeTokenNotice()).toBe('[clearing] remote access token is set');
    expect(all).toContain(bridgeTokenNotice());
    expect(all).not.toContain(KNOWN);
  });
});

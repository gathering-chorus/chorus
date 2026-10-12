// @test-type: unit — imports the module with a local stub script; no live service, no network
// @domain: code · @card: 4405 · owner: kade
/**
 * #4405 — importing server.ts must not start the board refresh. Every jest
 * file imports it; on 2026-10-12 12:40 each import launched `cards list`, and
 * in a werk with no built cards CLI each one compiled TypeScript: 63 at once,
 * load 144, the Clearing unreachable.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';

describe('#4405 board refresh starts only when server.ts is the server', () => {
  it('NEGATIVE PROOF — importing the module runs no `cards list`', async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'board-4405-'));
    const marker = path.join(dir, 'called');
    const stub = path.join(dir, 'cards');
    fs.writeFileSync(stub, `#!/bin/sh\necho "$@" >> ${marker}\n`, { mode: 0o755 });
    process.env.CHORUS_CARDS_BIN = stub;
    await import('../src/server');
    await new Promise((r) => setTimeout(r, 1500));
    expect(fs.existsSync(marker)).toBe(false);
  });
});

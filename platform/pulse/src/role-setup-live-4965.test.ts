// @domain: awareness
// @test-type: integration — liveReads in platform/pulse/src/role-setup.ts against a local door, a temp identity dir and a temp git repo; no live services
// @card: #4965
// @owner: silas
/**
 * #4965 — liveReads is what pulse actually runs for the #4919 setup check, and nothing
 * tested it: a door that answers, a key file's mode, a role settings file that differs
 * from git. Each read is shown both ways, so a read that always says "fine" goes red.
 */
import * as http from 'http';
import { execFileSync } from 'child_process';
import { promises as fs } from 'fs';
import * as os from 'os';
import * as path from 'path';
import { liveReads } from './role-setup';

let tmp = '';
beforeEach(async () => { tmp = await fs.mkdtemp(path.join(os.tmpdir(), 'role-setup-4965-')); });
afterEach(async () => { await fs.rm(tmp, { recursive: true, force: true }); });

const door = (status: number, body: unknown) => new Promise<{ url: string; close: () => Promise<void> }>((resolve) => {
  const s = http.createServer((req, res) => {
    res.statusCode = req.url?.startsWith('/v1/identity/principals') ? status : 404;
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify(body));
  });
  s.listen(0, '127.0.0.1', () => resolve({
    url: `http://127.0.0.1:${(s.address() as { port: number }).port}`,
    close: () => new Promise<void>((r) => s.close(() => r())),
  }));
});

describe('#4965 liveReads — the reads pulse runs for every role', () => {
  it('principals: names and host accounts from the door; a non-string field reads empty', async () => {
    const d = await door(200, { data: [{ name: 'kade', hostAccount: 'chorus-kade' }, { name: 7, hostAccount: null }] });
    try {
      expect(await liveReads(d.url, tmp, tmp).principals()).toEqual([
        { name: 'kade', hostAccount: 'chorus-kade' }, { name: '', hostAccount: '' },
      ]);
    } finally { await d.close(); }
  });

  it('NEGATIVE: a door that answers non-2xx throws with its status, never an empty roster', async () => {
    const d = await door(503, {});
    try {
      await expect(liveReads(d.url, tmp, tmp).principals()).rejects.toThrow('HTTP 503');
    } finally { await d.close(); }
  });

  it('keyMode: the file mode when nostr.json is there, null when it is not', async () => {
    await fs.mkdir(path.join(tmp, 'kade'));
    await fs.writeFile(path.join(tmp, 'kade', 'nostr.json'), '{}', { mode: 0o600 });
    await fs.chmod(path.join(tmp, 'kade', 'nostr.json'), 0o600);
    const reads = liveReads('http://127.0.0.1:9', tmp, tmp);
    expect((await reads.keyMode('kade'))! & 0o777).toBe(0o600);
    expect(await reads.keyMode('wren')).toBeNull();
  });

  it('NEGATIVE: keyMode refuses anything that is not a role name (no path out of the identity dir)', async () => {
    await expect(liveReads('http://127.0.0.1:9', tmp, tmp).keyMode('../etc')).rejects.toThrow('not a role name');
  });

  it('settingsDrift: false for a committed settings file, true once it differs from git', async () => {
    const git = (...a: string[]) => execFileSync('git', ['-C', tmp, ...a], { stdio: 'pipe' });
    git('init', '-q');
    git('config', 'user.email', 't@example.invalid'); git('config', 'user.name', 't');
    const rel = path.join('roles', 'kade', '.claude');
    await fs.mkdir(path.join(tmp, rel), { recursive: true });
    await fs.writeFile(path.join(tmp, rel, 'settings.json'), '{"a":1}\n');
    git('add', '.'); git('commit', '-q', '-m', 'seed');
    const reads = liveReads('http://127.0.0.1:9', tmp, tmp);
    expect(await reads.settingsDrift('kade')).toBe(false);
    await fs.writeFile(path.join(tmp, rel, 'settings.json'), '{"a":2}\n');
    expect(await reads.settingsDrift('kade')).toBe(true);
  });
});

// @test-type: fitness — parses chorus-api's real route table with the security sweep's own parser
// @card: #4417
// @owner: wren
/**
 * #4417 — the POST /api/cards/<verb> routes are gone. They had no caller and took
 * the filing role from a self-asserted X-Role header, so any credential holder
 * could file a card as Jeff past the bouncer. If one comes back, this goes red.
 */
import * as fs from 'fs';
import * as path from 'path';
import { parseMutationRoutes } from '../src/security-sweep';

const SERVER = fs.readFileSync(path.join(__dirname, '..', 'src', 'server.ts'), 'utf8');

describe('#4417 chorus-api has no card write routes', () => {
  test('no mutation route under /api/cards/ in server.ts', () => {
    const routes = parseMutationRoutes(SERVER).filter((r) => r.path.startsWith('/api/cards/'));
    expect(routes).toEqual([]);
  });

  test('NEGATIVE PROOF: the same parser finds such a route when one is there', () => {
    const planted = "app.post('/api/cards/add', async (req: Request, res: Response) => {});";
    expect(parseMutationRoutes(planted).map((r) => r.path)).toEqual(['/api/cards/add']);
  });

  test('the route table is really being read (target present, not vacuous)', () => {
    expect(parseMutationRoutes(SERVER).length).toBeGreaterThan(20);
  });
});

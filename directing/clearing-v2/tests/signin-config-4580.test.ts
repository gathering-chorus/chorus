// @test-type: unit — reads the shipped config file; no services
// @domain: messages
// @card: #4580
// @owner: wren
// #4580 — 2026-10-17 Jeff could not sign in: the shipped signInUrl pointed at the new
// Clearing itself, so "Sign in to send" went nowhere. It must be the common sign-in door,
// with the new Clearing as the place to come back to.
import fs from 'fs';
import path from 'path';

function signInIsTheDoor(url: string | undefined): string | null {
  if (!url) return 'no signInUrl';
  let u: URL;
  try { u = new URL(url); } catch { return `not a URL: ${url}`; }
  if (!u.pathname.startsWith('/_auth/')) return `not the common sign-in door: ${url}`;
  const next = u.searchParams.get('next') || '';
  if (!next.includes('/clearing-v2/')) return `does not come back to the new Clearing: ${url}`;
  return null;
}

test('the shipped config sends a signed-out visitor to the common door and back', () => {
  const cfg = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'clearing-v2.config.json'), 'utf8')) as { signInUrl?: string };
  expect(signInIsTheDoor(cfg.signInUrl)).toBeNull();
});

test('negative proof: the old self-pointing link fails the same check', () => {
  expect(signInIsTheDoor('https://lightlifeurbangardens.com/clearing-v2/')).toMatch(/not the common sign-in door/);
  expect(signInIsTheDoor('https://chorus.lightlifeurbangardens.com/_auth/?next=https%3A%2F%2Flightlifeurbangardens.com%2Fclearing%2F')).toMatch(/does not come back/);
});

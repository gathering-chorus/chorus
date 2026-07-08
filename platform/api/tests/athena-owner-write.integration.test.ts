// @test-type: integration — hits the live chorus-api at :3340; carries a scoped service token on writes (#3619).
/**
 * Tests for POST /api/athena/subdomains/:id/owner (#2508).
 *
 * Unit tests cover the patchTtlOwner regex (no I/O, no live API).
 * Integration tests hit the live API at localhost:3340 — gated on RUN_INTEGRATION.
 */
import { patchTtlOwner, findBlockTerminator } from '../src/handlers/athena-owner-write';
import { withServiceAuth } from './lib/service-token';
// #3619 — live mutation endpoints are envelope-secured; this suite is a real
// consumer and carries a scoped token on every write (deploy-before-require).
withServiceAuth();

const INTEGRATION_ENABLED = process.env.RUN_INTEGRATION === 'true';
const API = process.env.CHORUS_API || 'http://localhost:3340';

describe('patchTtlOwner', () => {
  const SAMPLE = `chorus:loom-decisions a chorus:SubDomain ;
    rdfs:label "Decisions" ;
    rdfs:comment "DEC-NNN — governing choices that constrain future behavior." ;
    chorus:ownedBy chorus:jeff ;
    chorus:primaryStep chorus:Shaping .

chorus:other-domain a chorus:SubDomain ;
    rdfs:label "Other" ;
    chorus:ownedBy chorus:silas ;
    chorus:primaryStep chorus:Building .
`;

  test('replaces owner within the matching subdomain block only', () => {
    const out = patchTtlOwner(SAMPLE, 'loom-decisions', 'wren');
    expect(out).not.toBeNull();
    expect(out).toContain('chorus:loom-decisions a chorus:SubDomain');
    expect(out).toContain('chorus:ownedBy chorus:wren');
    expect(out).toContain('chorus:other-domain a chorus:SubDomain');
    // Other block stays Silas
    const otherBlock = out!.split('chorus:other-domain')[1];
    expect(otherBlock).toContain('chorus:ownedBy chorus:silas');
    expect(otherBlock).not.toContain('chorus:ownedBy chorus:wren');
  });

  test('returns null when subdomain block missing', () => {
    expect(patchTtlOwner(SAMPLE, 'nonexistent-domain', 'wren')).toBeNull();
  });

  test('returns null when ownedBy line absent in block', () => {
    const noOwner = `chorus:naked-domain a chorus:SubDomain ;
    rdfs:label "Naked" ;
    chorus:primaryStep chorus:Shaping .
`;
    expect(patchTtlOwner(noOwner, 'naked-domain', 'wren')).toBeNull();
  });

  test('idempotent — applying same owner twice is a no-op on second call', () => {
    const first = patchTtlOwner(SAMPLE, 'loom-decisions', 'wren');
    const second = patchTtlOwner(first!, 'loom-decisions', 'wren');
    expect(second).toBe(first);
  });

  test('block with multi-line literal containing periods is not terminated early (gate:code Kade #2)', () => {
    const multiLine = `chorus:tricky-domain a chorus:SubDomain ;
    rdfs:label "Tricky" ;
    rdfs:comment "First sentence. Second sentence. Third." ;
    chorus:ownedBy chorus:jeff ;
    chorus:primaryStep chorus:Shaping .

chorus:other a chorus:SubDomain ;
    rdfs:label "Other" ;
    chorus:ownedBy chorus:silas ;
    chorus:primaryStep chorus:Building .
`;
    const out = patchTtlOwner(multiLine, 'tricky-domain', 'wren');
    expect(out).not.toBeNull();
    expect(out).toContain('chorus:tricky-domain');
    // Owner flipped on the right block
    const trickyBlock = out!.split('chorus:other')[0];
    expect(trickyBlock).toContain('chorus:ownedBy chorus:wren');
    expect(trickyBlock).toContain('"First sentence. Second sentence. Third."');
    // Other block untouched
    const otherBlock = out!.split('chorus:other')[1];
    expect(otherBlock).toContain('chorus:ownedBy chorus:silas');
  });

  test('findBlockTerminator skips over quoted literals (gate:code Kade #2)', () => {
    const ttl = `chorus:x a chorus:Foo ;
    rdfs:comment "has . periods . inside" ;
    chorus:ownedBy chorus:jeff .
chorus:y a chorus:Bar .
`;
    const end = findBlockTerminator(ttl, 0);
    // Should land after the chorus:x block's terminating period+newline,
    // i.e., at the start of "chorus:y"
    expect(end).toBeGreaterThan(0);
    expect(ttl.slice(end).startsWith('chorus:y')).toBe(true);
  });
});

let apiUp = false;
beforeAll(async () => {
  if (!INTEGRATION_ENABLED) return;
  try {
    const res = await fetch(`${API}/api/athena/health`);
    apiUp = res.ok;
  } catch {
    apiUp = false;
  }
});

const describeIntegration = INTEGRATION_ENABLED ? describe : describe.skip;

describeIntegration('POST /api/athena/subdomains/:id/owner', () => {
  test('rejects bad owner with 400', async () => {
    if (!apiUp) return;
    const res = await fetch(`${API}/api/athena/subdomains/loom-principles/owner`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ owner: 'bogus' }),
    });
    expect(res.status).toBe(400);
    const body = await res.json();
    expect(body.error).toContain('one of');
  });

  test('rejects missing owner with 400', async () => {
    if (!apiUp) return;
    const res = await fetch(`${API}/api/athena/subdomains/loom-principles/owner`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
  });

  test('returns 404 for unknown subdomain', async () => {
    if (!apiUp) return;
    const res = await fetch(`${API}/api/athena/subdomains/this-subdomain-does-not-exist/owner`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ owner: 'wren' }),
    });
    expect(res.status).toBe(404);
  });

  test('idempotent re-assign of existing owner returns 200', async () => {
    if (!apiUp) return;
    // loom-principles is already owned by wren (per #2508 data fix)
    const res = await fetch(`${API}/api/athena/subdomains/loom-principles/owner`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ owner: 'wren' }),
    });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.subdomain).toBe('loom-principles');
    expect(body.owner).toBe('wren');
  });

  test('graph state actually changes after POST (gate:code Kade #3.b — closes WHERE-drift loophole)', async () => {
    if (!apiUp) return;
    // Flip owner via API, then SELECT via /api/athena/subdomains and confirm
    const target = 'loom-decisions';
    const flipTo = 'silas';
    const restoreTo = 'wren';

    const flipRes = await fetch(`${API}/api/athena/subdomains/${target}/owner`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ owner: flipTo }),
    });
    expect(flipRes.status).toBe(200);

    // Verify by querying the live graph (not just trusting handler response)
    const verifyRes = await fetch(`${API}/api/athena/subdomains?owner=Silas`);
    const verifyBody = await verifyRes.json();
    const labels = (verifyBody.data || []).map((it: { label: string }) => it.label);
    expect(labels).toContain('Decisions');

    // Restore so subsequent test runs / live demo state is preserved
    const restoreRes = await fetch(`${API}/api/athena/subdomains/${target}/owner`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ owner: restoreTo }),
    });
    expect(restoreRes.status).toBe(200);
  });
});

// @test-type: unit — pure patchTtlOwner/findBlockTerminator cases; no I/O.
/**
 * Tests for POST /api/athena/subdomains/:id/owner (#2508).
 *
 * Unit tests cover the patchTtlOwner regex (no I/O, no live API).
 *
 * #4113 — the live-API block that used to follow was retired: it ran as the
 * runner's principal (no athena write scope, so the door answered 403 before
 * validation, a red every night), and it flipped prod ownership of
 * loom-decisions + rewrote canonical roles/silas/ontology/chorus.ttl — a test
 * writing production. The 400/404/200 handler paths are pinned in
 * tests/handlers/athena-owner-write.test.ts with fake deps.
 */
import { patchTtlOwner, findBlockTerminator } from '../src/handlers/athena-owner-write';

// #4274: fixtures type chorus:Domain — chorus:SubDomain is retired (#4265) and the
// live rows this patcher edits are Domain rows.
describe('patchTtlOwner', () => {
  const SAMPLE = `chorus:loom-decisions a chorus:Domain ;
    rdfs:label "Decisions" ;
    rdfs:comment "DEC-NNN — governing choices that constrain future behavior." ;
    chorus:ownedBy chorus:jeff ;
    chorus:primaryStep chorus:Shaping .

chorus:other-domain a chorus:Domain ;
    rdfs:label "Other" ;
    chorus:ownedBy chorus:silas ;
    chorus:primaryStep chorus:Building .
`;

  test('replaces owner within the matching subdomain block only', () => {
    const out = patchTtlOwner(SAMPLE, 'loom-decisions', 'wren');
    expect(out).not.toBeNull();
    expect(out).toContain('chorus:loom-decisions a chorus:Domain');
    expect(out).toContain('chorus:ownedBy chorus:principal-wren'); // #4274: owners are principals
    expect(out).toContain('chorus:other-domain a chorus:Domain');
    // Other block stays Silas
    const otherBlock = out!.split('chorus:other-domain')[1];
    expect(otherBlock).toContain('chorus:ownedBy chorus:silas');
    expect(otherBlock).not.toContain('chorus:ownedBy chorus:principal-wren');
  });

  test('a block typed chorus:SubDomain is not patched — the class is retired (#4265/#4274)', () => {
    const retired = SAMPLE.replace('chorus:loom-decisions a chorus:Domain', 'chorus:loom-decisions a chorus:SubDomain');
    expect(patchTtlOwner(retired, 'loom-decisions', 'wren')).toBeNull();
  });

  test('returns null when subdomain block missing', () => {
    expect(patchTtlOwner(SAMPLE, 'nonexistent-domain', 'wren')).toBeNull();
  });

  test('returns null when ownedBy line absent in block', () => {
    const noOwner = `chorus:naked-domain a chorus:Domain ;
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
    const multiLine = `chorus:tricky-domain a chorus:Domain ;
    rdfs:label "Tricky" ;
    rdfs:comment "First sentence. Second sentence. Third." ;
    chorus:ownedBy chorus:jeff ;
    chorus:primaryStep chorus:Shaping .

chorus:other a chorus:Domain ;
    rdfs:label "Other" ;
    chorus:ownedBy chorus:silas ;
    chorus:primaryStep chorus:Building .
`;
    const out = patchTtlOwner(multiLine, 'tricky-domain', 'wren');
    expect(out).not.toBeNull();
    expect(out).toContain('chorus:tricky-domain');
    // Owner flipped on the right block
    const trickyBlock = out!.split('chorus:other')[0];
    expect(trickyBlock).toContain('chorus:ownedBy chorus:principal-wren');
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

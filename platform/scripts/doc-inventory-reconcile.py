#!/usr/bin/env python3
"""doc-inventory-reconcile.py — diff /api/doc-catalog vs doc-inventory.tsv.

Output:
  - counts: catalog entries, tsv rows, in-catalog-not-tsv, in-tsv-not-catalog
  - per-source breakdown for each set
  - exit 1 if catalog entries that should be in tsv but aren't (excluding
    documented exclusion classes) — for use as a CI ratchet

Documented exclusions (catalog entries intentionally not in tsv):
  - filenames in TSV_NAME_EXCLUDES (mirrors doc-inventory.sh -not -name list)
  - 'manual' source — catalog-only, no filesystem origin

Run: python3 doc-inventory-reconcile.py [--strict]
"""
import json, os, sys, urllib.request

GATHERING = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site'
CHORUS = '/Users/jeffbridwell/CascadeProjects/chorus'
TSV = f'{CHORUS}/knowledge/doc-inventory.tsv'
CATALOG_URL = 'http://localhost:3340/api/doc-catalog'

# catalog source label → (repo, relative dir under repo root)
SOURCE_MAP = {
    'public': ('gathering', 'public'),
    'gathering-docs': ('gathering', 'public/gathering-docs'),
    'chorus-docs': ('gathering', 'public/chorus-docs'),
    'akasha': ('gathering', 'public/akasha'),
    'docs': ('gathering', 'docs'),
    'data/about': ('gathering', 'data/about'),
    'wren/artifacts': ('chorus', 'roles/wren/artifacts'),
    'wren/docs': ('chorus', 'roles/wren/docs'),
    'wren/decisions': ('chorus', 'roles/wren/decisions'),
    'architect/docs': ('chorus', 'roles/silas/docs'),
    'architect/artifacts': ('chorus', 'roles/silas/artifacts'),
    'architect/adr': ('chorus', 'roles/silas/adr'),
    'designing/docs': ('chorus', 'designing/docs'),
    'designing/decisions': ('chorus', 'designing/decisions'),
    'docs/diagrams': ('chorus', 'docs/diagrams'),
}

# Filenames excluded by doc-inventory.sh walk_repo (-not -name ...)
TSV_NAME_EXCLUDES = {
    'CLAUDE.md', 'backlog.md', 'projects.md', 'stories.md', 'tech-debt.md',
    'decisions.md', 'service-manifest.md', 'scope-ownership.md',
    'role-config-manifest.md', 'RUNBOOK.md', 'RUNBOOK.html',
    'TEAM_PROTOCOL.md', 'team-architecture.md', 'README.md',
    'TEST.md', 'test-triage.md', 'reference-templates.md',
    'turtle-filesystem-and-ontology.md', 'next-session.md',
    'next-session.md.consumed',
}


def resolve_catalog_path(href, source):
    info = SOURCE_MAP.get(source)
    if info is None:
        return None
    repo, dir_ = info
    fn = href.rstrip('/').split('/')[-1]
    base = GATHERING if repo == 'gathering' else CHORUS
    for ext in ['', '.md', '.html']:
        p = os.path.join(base, dir_, fn + ext)
        if os.path.exists(p):
            return (repo, os.path.relpath(p, base))
    return None


def load_tsv():
    rows = []
    with open(TSV) as f:
        for line in f:
            if line.startswith('#') or not line.strip():
                continue
            parts = line.rstrip('\n').split('\t')
            if len(parts) >= 2:
                rows.append((parts[0], parts[1]))
    return rows


def main():
    catalog = json.loads(urllib.request.urlopen(CATALOG_URL).read())
    cat_entries = [
        (doc.get('href',''), doc.get('source',''))
        for g in catalog.get('groups', [])
        for doc in g.get('docs', [])
    ]

    expected = set()
    unmapped = []
    for href, src in cat_entries:
        info = resolve_catalog_path(href, src)
        if info is None:
            unmapped.append((src, href))
        else:
            expected.add(info)

    tsv_set = set(load_tsv())

    cat_only = expected - tsv_set
    tsv_only = tsv_set - expected
    cat_only_excluded = {(r,p) for (r,p) in cat_only if os.path.basename(p) in TSV_NAME_EXCLUDES}
    cat_only_real = cat_only - cat_only_excluded

    print(f'Catalog entries:            {len(cat_entries)}')
    print(f'  → resolved to fs paths:   {len(expected)}')
    print(f'  → unmapped sources:       {len(unmapped)}')
    print(f'TSV rows:                   {len(tsv_set)}')
    print()
    print(f'In catalog, not in tsv:     {len(cat_only)} total')
    print(f'  → name-excluded by tsv:   {len(cat_only_excluded)} (documented exclusion)')
    print(f'  → real misses:            {len(cat_only_real)}')
    print(f'In tsv, not in catalog:     {len(tsv_only)}')

    if cat_only_real:
        print('\nREAL MISSES — catalog entries tsv should cover but doesn\'t:')
        for r, p in sorted(cat_only_real):
            print(f'  {r}: {p}')
    if unmapped:
        print('\nUnmapped catalog sources (add to SOURCE_MAP):')
        for src, href in unmapped:
            print(f'  source={src!r} href={href}')

    # In-tsv-not-catalog: bucket by reason
    print('\n--- TSV-only breakdown ---')
    siblings = archived = role_internal = 0
    other = []
    for r, p in tsv_only:
        if '/_archived/' in p:
            archived += 1
            continue
        base, ext = os.path.splitext(p)
        other_ext = '.html' if ext == '.md' else '.md'
        if (r, base + other_ext) in expected:
            siblings += 1
            continue
        if p.startswith('roles/') and any(rl in p for rl in ['silas/', 'wren/', 'kade/']):
            role_internal += 1
            continue
        other.append((r, p))
    print(f'  Sibling-format (tsv has .md/.html, catalog has the other): {siblings}')
    print(f'  Under _archived/:                                          {archived}')
    print(f'  Role-internal (roles/silas|wren|kade):                     {role_internal}')
    print(f'  Other:                                                     {len(other)}')
    if other:
        for r, p in other[:15]:
            print(f'    {r}: {p}')
        if len(other) > 15:
            print(f'    ... and {len(other)-15} more')

    if '--strict' in sys.argv and cat_only_real:
        sys.exit(1)


if __name__ == '__main__':
    main()

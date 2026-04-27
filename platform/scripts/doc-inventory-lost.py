#!/usr/bin/env python3
"""doc-inventory-lost.py — file-exists verification.

Reads doc-inventory.tsv, checks every path with fs.statSync, prints the rows
where the file no longer exists on disk. These are 'lost' — the tsv used to
have them but the filesystem no longer does.

Use case: after a migration or cleanup, surface what got deleted so it doesn't
silently disappear from the audit. Pair with the migration-loss class
(yesterday's #44/46/49/50/63/66 from inventory).

Run: python3 doc-inventory-lost.py
"""
import os, sys

GATHERING = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site'
CHORUS = '/Users/jeffbridwell/CascadeProjects/chorus'
TSV = f'{CHORUS}/knowledge/doc-inventory.tsv'

def main():
    lost = []
    total = 0
    with open(TSV) as f:
        for line in f:
            if line.startswith('#') or not line.strip():
                continue
            parts = line.rstrip('\n').split('\t')
            if len(parts) < 2:
                continue
            repo, rel = parts[0], parts[1]
            total += 1
            base = GATHERING if repo == 'gathering' else CHORUS
            abs_p = os.path.join(base, rel)
            if not os.path.exists(abs_p):
                lost.append((repo, rel))

    print(f'TSV rows checked: {total}')
    print(f'Lost (file not on disk): {len(lost)}')
    if lost:
        print()
        for r, p in lost:
            print(f'  {r}: {p}')

if __name__ == '__main__':
    main()

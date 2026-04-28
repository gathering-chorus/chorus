#!/usr/bin/env python3
"""doc-inventory-state.py — populate state column from rule.

Reads doc-inventory.tsv and recomputes column 3 (state) from rules:

  state = lost           if file does not exist on disk
  state = misfiled       if classification != repo (cross-product)
                          AND classification in {chorus,gathering}
                          (akasha and archive are first-class buckets)
  state = unfiled        if in-catalog flag is N AND not under a
                          catalog source dir
  state = ok             otherwise

Writes back to tsv (in-place, with backup at <path>.bak).

Run: python3 doc-inventory-state.py [--dry-run]
"""
import os, sys, shutil
from collections import Counter

GATHERING = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site'
CHORUS = '/Users/jeffbridwell/CascadeProjects/chorus'
TSV = f'{CHORUS}/knowledge/doc-inventory.tsv'


def derive_state(repo, rel, classification, in_catalog):
    base = GATHERING if repo == 'gathering' else CHORUS
    abs_p = os.path.join(base, rel)
    if not os.path.exists(abs_p):
        return 'lost'
    if classification in ('chorus', 'gathering') and classification != repo:
        return 'misfiled'
    if in_catalog == 'N':
        return 'unfiled'
    return 'ok'


def main():
    dry = '--dry-run' in sys.argv
    rows = []
    counts = Counter()
    misfiled_examples = []
    with open(TSV) as f:
        for line in f:
            if not line.strip():
                continue
            parts = line.rstrip('\n').split('\t')
            if len(parts) < 6:
                rows.append(line.rstrip('\n'))
                continue
            repo, rel, _state, classification = parts[0], parts[1], parts[2], parts[3]
            in_catalog = parts[5]
            new_state = derive_state(repo, rel, classification, in_catalog)
            parts[2] = new_state
            counts[new_state] += 1
            if new_state == 'misfiled' and len(misfiled_examples) < 10:
                misfiled_examples.append((repo, rel, classification))
            rows.append('\t'.join(parts))

    print('State distribution:')
    for s, n in counts.most_common():
        print(f'  {n:4d}  {s}')

    if misfiled_examples:
        print('\nMisfiled examples (chorus content in gathering repo, or vice versa):')
        for r, p, c in misfiled_examples:
            print(f'  {r}: {p}  → should be {c}')

    if dry:
        print('\n(dry-run, tsv unchanged)')
        return

    shutil.copy(TSV, TSV + '.bak')
    with open(TSV, 'w') as f:
        for line in rows:
            f.write(line + '\n')
    print(f'\nWrote {len(rows)} rows to {TSV}')


if __name__ == '__main__':
    main()

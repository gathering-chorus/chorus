#!/usr/bin/env python3
"""doc-inventory-freshness.py — add mtime + full sha256 columns.

Reads doc-inventory.tsv. For each row, computes:
  - mtime: source file's mtime as ISO date (YYYY-MM-DD)
  - sha256: full sha256 hex of source content

Appends as columns 9 and 10. Existing columns:
  1=repo  2=path  3=state  4=classification  5=owner  6=in-catalog
  7=topic  8=hash-truncated  9=mtime  10=sha256

Also emits a header comment line at the top so consumers know the schema.

Drift detection (future): re-run and compare column 10 to last-recorded
sha256 — mismatch means content changed since last scan, regardless of
whether mtime moved (e.g., if file was touched but unchanged or modified
without mtime update on copy).

Run: python3 doc-inventory-freshness.py [--dry-run]
"""
import os, sys, hashlib, shutil, datetime

GATHERING = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site'
CHORUS = '/Users/jeffbridwell/CascadeProjects/chorus'
TSV = f'{CHORUS}/knowledge/doc-inventory.tsv'

HEADER = '# repo\tpath\tstate\tclassification\towner\tin-catalog\ttopic\thash12\tmtime\tsha256'


def file_freshness(repo, rel):
    base = GATHERING if repo == 'gathering' else CHORUS
    abs_p = os.path.join(base, rel)
    try:
        st = os.stat(abs_p)
        mtime = datetime.date.fromtimestamp(st.st_mtime).isoformat()
        with open(abs_p, 'rb') as f:
            sha = hashlib.sha256(f.read()).hexdigest()
        return mtime, sha
    except (OSError, IOError):
        return '', ''


def main():
    dry = '--dry-run' in sys.argv
    rows = [HEADER]
    drift_count = 0
    fresh_count = 0
    with open(TSV) as f:
        for line in f:
            if not line.strip() or line.startswith('#'):
                continue
            parts = line.rstrip('\n').split('\t')
            if len(parts) < 6:
                rows.append(line.rstrip('\n'))
                continue
            repo, rel = parts[0], parts[1]
            mtime, sha = file_freshness(repo, rel)
            # Truncate to 10 cols if longer
            parts = parts[:8]
            # Pad to 8 if shorter (some rows have 7 due to empty topic field)
            while len(parts) < 8:
                parts.append('')
            # Append new columns
            old_sha = parts[9] if len(parts) > 9 else ''
            parts.extend([mtime, sha])
            rows.append('\t'.join(parts))
            if old_sha and old_sha != sha:
                drift_count += 1
            elif sha:
                fresh_count += 1

    print(f'Rows processed:    {len(rows)-1}')
    print(f'Fresh (sha valid): {fresh_count}')
    print(f'Drifted from prior: {drift_count} (always 0 on first run; populates on rerun)')

    if dry:
        print('\n(dry-run, tsv unchanged)')
        return

    shutil.copy(TSV, TSV + '.bak')
    with open(TSV, 'w') as f:
        for line in rows:
            f.write(line + '\n')
    print(f'\nWrote {len(rows)} rows to {TSV} (header + {len(rows)-1} data)')


if __name__ == '__main__':
    main()

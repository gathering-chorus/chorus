#!/usr/bin/env python3
"""doc-inventory-classify.py — apply multi-signal classification to inventory.

Reads doc-inventory.tsv, recomputes the `classification` column for every row
using three signals in priority order:

  1. Path-based: explicit dir → product mapping (highest confidence)
  2. Filename prefix: existing chorus-/gathering-/borg-/etc. patterns
  3. Content keyword: scan first 50 lines, count chorus vs gathering mentions

Writes back to tsv (in-place, with backup at <path>.bak).

Output buckets: chorus | gathering | akasha | archive | lost | ambiguous
(ambiguous remains only when ALL three signals disagree or are absent;
target zero on the current corpus.)

Run: python3 doc-inventory-classify.py [--dry-run]
"""
import os, re, sys, shutil

GATHERING = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site'
CHORUS = '/Users/jeffbridwell/CascadeProjects/chorus'
TSV = f'{CHORUS}/knowledge/doc-inventory.tsv'

# Path → product. Most-specific match wins (longer prefix beats shorter).
PATH_RULES = [
    # chorus side
    ('chorus', 'roles/silas/', 'chorus'),
    ('chorus', 'roles/wren/', 'chorus'),
    ('chorus', 'roles/kade/', 'chorus'),
    ('chorus', 'designing/', 'chorus'),
    ('chorus', 'platform/', 'chorus'),
    ('chorus', 'docs/diagrams/', 'chorus'),
    ('chorus', 'building/', 'chorus'),
    ('chorus', 'knowledge/', 'chorus'),
    # gathering side — explicit
    ('gathering', 'public/akasha/', 'akasha'),
    ('gathering', 'public/gathering-docs/', 'gathering'),
    ('gathering', 'public/chorus-docs/', 'chorus'),
    ('gathering', 'public/docs/', 'gathering'),
    ('gathering', 'data/about/_archived/', 'archive'),
    # public/ root and docs/ are gathering-by-default (legacy)
    ('gathering', 'docs/', 'gathering'),
    ('gathering', 'public/', 'gathering'),
    # data/about is mixed-content (chorus + gathering both write here);
    # fall through to filename + content signals.
]

FILENAME_PREFIX = re.compile(r'^(chorus-|borg-|silas-|wren-|kade-|adr-|sequence-|icd-)', re.I)
GATHERING_PREFIX = re.compile(r'^(gathering-|site-|garden-|photo-|photos-|home-|blog-)', re.I)
# Substring matches — catch UPPERCASE_NAMES like CHORUS_COMMAND_CARD.html
CHORUS_TOKENS = re.compile(r'(chorus|borg|wren|silas|kade|werk|loom|athena|convergence)', re.I)
GATHERING_TOKENS = re.compile(r'(gathering|garden|blog|photo|wordpress|self_portrait|owner_persona)', re.I)


def classify_by_path(repo, rel):
    rel_lower = rel.lower()
    best = (None, 0)
    for r, prefix, product in PATH_RULES:
        if r == repo and rel_lower.startswith(prefix):
            if len(prefix) > best[1]:
                best = (product, len(prefix))
    return best[0]


def classify_by_filename(rel):
    bn = os.path.basename(rel).lower()
    if FILENAME_PREFIX.match(bn):
        return 'chorus'
    if GATHERING_PREFIX.match(bn):
        return 'gathering'
    # Substring match for UPPERCASE_TOKEN style names
    chorus_hit = bool(CHORUS_TOKENS.search(bn))
    gathering_hit = bool(GATHERING_TOKENS.search(bn))
    if chorus_hit and not gathering_hit:
        return 'chorus'
    if gathering_hit and not chorus_hit:
        return 'gathering'
    return None


def classify_by_content(repo, rel):
    base = GATHERING if repo == 'gathering' else CHORUS
    abs_p = os.path.join(base, rel)
    try:
        with open(abs_p, errors='ignore') as f:
            # Read up to 8KB — covers most prose docs without runaway on huge files
            head = f.read(8192).lower()
    except (OSError, IOError):
        return None

    # Count both single-word and tokens
    chorus_score = (head.count('chorus') + head.count('borg') + head.count('werk')
                    + head.count('wren') + head.count('silas') + head.count('kade'))
    gathering_score = (head.count('gathering') + head.count('garden') + head.count('blog')
                       + head.count('photo') + head.count('wordpress'))
    # Need a clear winner (>= 2x) with at least 2 hits to fire
    if chorus_score >= 2 and chorus_score >= 2 * gathering_score:
        return 'chorus'
    if gathering_score >= 2 and gathering_score >= 2 * chorus_score:
        return 'gathering'
    return None


def classify(repo, rel):
    """Returns (classification, signal). Signal is which rule fired."""
    p = classify_by_path(repo, rel)
    if p:
        return p, 'path'
    f = classify_by_filename(rel)
    if f:
        return f, 'filename'
    c = classify_by_content(repo, rel)
    if c:
        return c, 'content'
    # Tie-breaker: data/about/ is the chorus-content cabinet hosted in gathering
    # app. When all signals fail, default to chorus and leave the misfiled
    # detector (cross-product audit) to flag if content disagrees.
    if repo == 'gathering' and rel.startswith('data/about/'):
        return 'chorus', 'fallback-data-about'
    return 'ambiguous', 'none'


def main():
    dry = '--dry-run' in sys.argv

    rows = []
    with open(TSV) as f:
        for line in f:
            if not line.strip():
                continue
            parts = line.rstrip('\n').split('\t')
            if len(parts) < 4:
                rows.append(line.rstrip('\n'))
                continue
            repo, rel, state = parts[0], parts[1], parts[2]
            new_class, signal = classify(repo, rel)
            parts[3] = new_class
            rows.append('\t'.join(parts))

    # Counts
    from collections import Counter
    counts = Counter()
    signals = Counter()
    for line in rows:
        parts = line.split('\t')
        if len(parts) >= 4:
            counts[parts[3]] += 1
            # Recompute for signal stat
            repo, rel = parts[0], parts[1]
            _, sig = classify(repo, rel)
            signals[sig] += 1

    print('Classification distribution:')
    for c, n in counts.most_common():
        print(f'  {n:4d}  {c}')
    print()
    print('Signal breakdown:')
    for s, n in signals.most_common():
        print(f'  {n:4d}  {s}')

    if dry:
        print('\n(dry-run, tsv unchanged)')
        return

    shutil.copy(TSV, TSV + '.bak')
    with open(TSV, 'w') as f:
        for line in rows:
            f.write(line + '\n')
    print(f'\nWrote {len(rows)} rows to {TSV} (backup: {TSV}.bak)')


if __name__ == '__main__':
    main()

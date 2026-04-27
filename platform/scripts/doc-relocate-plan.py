#!/usr/bin/env python3
"""doc-relocate-plan.py — map misfiled docs to their chorus-repo destinations.

Reads doc-inventory.tsv, takes every state=misfiled row from the gathering
repo, classifies each by filename + content cues, and emits a migration plan:

  source_path  →  chorus_destination_dir
  + collision flag if a file with the same name already exists at target

Doesn't move anything. Use --execute to perform the moves after review.

Targets in chorus repo:
  roles/silas/adr/        — ADR-NNN-*.md (architectural decision records)
  designing/docs/         — service designs, *_ARCHITECTURE, *_PATTERN, *_MODEL
  designing/decisions/    — DEC-* files, decisions.md-style records
  docs/diagrams/          — pure diagrams (mermaid, image-heavy)
  knowledge/              — taxonomies, indexes, manifests

Run: python3 doc-relocate-plan.py            # report only
     python3 doc-relocate-plan.py --execute  # move files (uses git mv where possible)
"""
import os, re, sys, shutil, subprocess

GATHERING = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site'
CHORUS = '/Users/jeffbridwell/CascadeProjects/chorus'
TSV = f'{CHORUS}/knowledge/doc-inventory.tsv'


def classify_target(rel_path: str, basename: str) -> str:
    bn_lower = basename.lower()
    # ADR files → roles/silas/adr/ (Silas owns ADRs)
    if re.match(r'^adr-\d+', bn_lower):
        return 'roles/silas/adr'
    # DEC files / decision records → designing/decisions
    if re.match(r'^dec-\d+', bn_lower) or bn_lower == 'decisions.md':
        return 'designing/decisions'
    # Service designs, architecture, patterns, research → designing/docs
    arch_tokens = (
        'architecture', 'pattern', 'model', 'service.design', 'manifest',
        'topology', 'foundations', 'horizontal', 'guardrails', 'completeness',
        'taxonomy', 'sdk', 'borg', 'living', 'emergent', 'memory', 'attention',
        'cadence', 'log_relatedness', 'log_topology', 'wardley', 'icd_',
        'homeostasis', 'engineering_horizontal', 'pair_programming',
        'system_model', 'product_taxonomy', 'startup_process', 'good_borg',
        'cheat_sheet', 'chorus_command_card', 'next_sequence', 'posture_strip',
        'kade_role', 'interaction_architecture', 'interaction_patterns',
        'gallery-refactoring', 'spine-observability', 'system-model-thinking',
    )
    if any(tok in bn_lower for tok in arch_tokens):
        return 'designing/docs'
    # Process / nudge / bridge → designing/docs
    if any(tok in bn_lower for tok in ('nudge', 'bridge', 'testing', 'solid')):
        return 'designing/docs'
    # README → designing/docs/chorus-readme.md (rename to avoid root collision)
    if 'readme' in bn_lower:
        return 'designing/docs'
    # Infrastructure → designing/docs
    if 'infrastructure' in bn_lower:
        return 'designing/docs'
    # Default
    return 'designing/docs'


def derive_target_filename(basename: str, target_dir: str) -> str:
    """Some files need rename to avoid collisions or reflect new home."""
    bn_lower = basename.lower()
    # CHORUS_README.md is too generic for designing/docs/ — rename
    if bn_lower == 'chorus_readme.md':
        return 'chorus-readme.md'
    # DECISIONS.md is too generic — clarify
    if bn_lower == 'decisions.md':
        return 'gathering-data-about-decisions.md'
    return basename


def main():
    execute = '--execute' in sys.argv
    moves = []
    with open(TSV) as f:
        for line in f:
            if line.startswith('#') or not line.strip():
                continue
            parts = line.rstrip('\n').split('\t')
            if len(parts) < 4:
                continue
            repo, rel, state = parts[0], parts[1], parts[2]
            if state != 'misfiled' or repo != 'gathering':
                continue
            basename = os.path.basename(rel)
            target_dir = classify_target(rel, basename)
            target_name = derive_target_filename(basename, target_dir)
            target_rel = f'{target_dir}/{target_name}'
            target_abs = os.path.join(CHORUS, target_rel)
            source_abs = os.path.join(GATHERING, rel)
            collision = os.path.exists(target_abs)
            moves.append({
                'source_rel': rel,
                'source_abs': source_abs,
                'target_rel': target_rel,
                'target_abs': target_abs,
                'collision': collision,
            })

    # Report
    from collections import Counter
    bucket_counts = Counter(m['target_rel'].rsplit('/', 1)[0] for m in moves)
    print(f'Misfiled docs to relocate: {len(moves)}')
    print('\nDestination buckets:')
    for d, n in bucket_counts.most_common():
        print(f'  {n:3d}  → chorus/{d}/')

    collisions = [m for m in moves if m['collision']]
    print(f'\nCollisions (target exists): {len(collisions)}')
    for m in collisions:
        print(f'  {m["source_rel"]}  →  {m["target_rel"]}  ⚠ collision')

    if not execute:
        print('\nFull plan:')
        for m in moves:
            flag = ' ⚠' if m['collision'] else ''
            print(f'  {m["source_rel"]}  →  {m["target_rel"]}{flag}')
        print(f'\n(dry-run; pass --execute to perform moves)')
        return

    # Execute moves. Collision policy:
    #   - If chorus version mtime >= gathering version mtime → delete gathering (stale dupe)
    #   - If gathering is newer → skip with warning, manual review needed
    moved = 0
    deleted_dupes = 0
    skipped = 0
    for m in moves:
        if m['collision']:
            try:
                src_mtime = os.path.getmtime(m['source_abs'])
                tgt_mtime = os.path.getmtime(m['target_abs'])
                if tgt_mtime >= src_mtime:
                    os.remove(m['source_abs'])
                    print(f'DELETE-DUPE (chorus is canonical): {m["source_rel"]}')
                    deleted_dupes += 1
                else:
                    print(f'SKIP (gathering newer, manual review): {m["source_rel"]}')
                    skipped += 1
            except OSError as e:
                print(f'FAIL collision-handle {m["source_rel"]}: {e}')
                skipped += 1
            continue
        os.makedirs(os.path.dirname(m['target_abs']), exist_ok=True)
        try:
            shutil.copy2(m['source_abs'], m['target_abs'])
            os.remove(m['source_abs'])
            print(f'MOVED: {m["source_rel"]}  →  {m["target_rel"]}')
            moved += 1
        except (OSError, IOError) as e:
            print(f'FAIL: {m["source_rel"]}: {e}')
    print(f'\nMoved: {moved}, Deleted-dupes: {deleted_dupes}, Skipped: {skipped}')


if __name__ == '__main__':
    main()

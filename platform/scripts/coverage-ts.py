#!/usr/bin/env python3
"""TypeScript coverage aggregator for chorus (#2197).

Walks every project with a package.json that has a jest config, counts
every .ts source statement across all source files, counts covered
statements from the coverage-final.json produced by `jest --coverage`,
and reports one deterministic number: covered / total.

The key difference from jest's built-in "All files" summary: jest only
counts files in the coverage denominator if SOME test imported them.
This script counts every .ts source file regardless of whether any
test touched it — the number reflects real coverage of the codebase,
not the test-visible subset.

Usage:
    coverage-ts.py [--run]

    --run   run `jest --coverage --coverageReporters=json-summary`
            per project before aggregating (slower). Default is to
            use the coverage-final.json already on disk from the last
            jest --coverage invocation.

Output (stdout, one line per project plus totals):
    project      src_stmts  tested  covered  real_pct
    ...
    TOTAL        N          N       N        X.X%
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

CHORUS_ROOT = Path(os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"))

PROJECTS = [
    "directing/clearing",
    "directing/products/cards",
    "platform/workflow-engine",
    "platform/tests",
    "platform/chorus-sdk",
    "platform/pulse",
    "platform/api",
]

# Statement-like pattern: count non-blank, non-comment-only lines in .ts files
# outside tests. Uses line-count as a proxy for jest's statement count — the
# ratio is stable enough (lines ≈ statements in TS) and deterministic per
# commit. True AST-based counts would require ts-morph; line-count ties the
# denominator to something the script computes itself.
SOURCE_EXTS = {".ts"}
EXCLUDE_SUFFIXES = (".d.ts", ".test.ts", ".spec.ts")
EXCLUDE_DIRS = {"node_modules", "dist", "coverage", ".next", "build"}


def count_source_statements(project_root: Path) -> int:
    """Count non-blank, non-pure-comment lines in .ts source files."""
    src_dir = project_root / "src"
    if not src_dir.is_dir():
        return 0
    total = 0
    for root, dirs, files in os.walk(src_dir):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]
        for f in files:
            if not f.endswith(".ts"):
                continue
            if any(f.endswith(s) for s in EXCLUDE_SUFFIXES):
                continue
            path = Path(root) / f
            try:
                with path.open("r", errors="replace") as fh:
                    for line in fh:
                        stripped = line.strip()
                        if not stripped:
                            continue
                        if stripped.startswith("//") or stripped.startswith("*") or stripped.startswith("/*"):
                            continue
                        total += 1
            except OSError:
                continue
    return total


def read_covered_statements(project_root: Path) -> tuple[int, int]:
    """Read jest's coverage-final.json; return (covered_stmts, stmts_in_tested_files)."""
    cov_path = project_root / "coverage" / "coverage-final.json"
    if not cov_path.is_file():
        return 0, 0
    try:
        with cov_path.open("r") as fh:
            cov = json.load(fh)
    except (OSError, json.JSONDecodeError):
        return 0, 0
    tested = 0
    covered = 0
    for _, data in cov.items():
        statements = data.get("s", {})
        tested += len(statements)
        covered += sum(1 for v in statements.values() if isinstance(v, int) and v > 0)
    return covered, tested


def run_jest_coverage(project_root: Path) -> None:
    """Run `npx jest --coverage` in the project. Leaves coverage-final.json on disk."""
    try:
        subprocess.run(
            ["npx", "jest", "--coverage", "--silent", "--coverageReporters=json"],
            cwd=project_root,
            check=False,
            capture_output=True,
            timeout=300,
        )
    except (subprocess.TimeoutExpired, OSError):
        pass


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="store_true", help="run jest --coverage per project first")
    ap.add_argument("--json", action="store_true", help="emit machine-readable JSON to stdout")
    args = ap.parse_args()

    rows = []
    total_src = 0
    total_tested = 0
    total_covered = 0

    for proj_rel in PROJECTS:
        proj_root = CHORUS_ROOT / proj_rel
        if not proj_root.is_dir():
            continue
        if args.run:
            run_jest_coverage(proj_root)
        src_stmts = count_source_statements(proj_root)
        covered, tested = read_covered_statements(proj_root)
        total_src += src_stmts
        total_tested += tested
        total_covered += covered
        real_pct = 100.0 * covered / src_stmts if src_stmts else 0.0
        rows.append({
            "project": proj_rel,
            "src_stmts": src_stmts,
            "tested": tested,
            "covered": covered,
            "real_pct": round(real_pct, 2),
        })

    real_total = 100.0 * total_covered / total_src if total_src else 0.0

    if args.json:
        print(json.dumps({
            "language": "ts",
            "projects": rows,
            "total": {"src_stmts": total_src, "tested": total_tested, "covered": total_covered, "real_pct": round(real_total, 2)},
        }, indent=2))
        return 0

    print(f"{'project':32s} {'src':>8s} {'tested':>8s} {'covered':>8s} {'real %':>8s}")
    print("-" * 72)
    for r in rows:
        print(f"{r['project']:32s} {r['src_stmts']:>8d} {r['tested']:>8d} {r['covered']:>8d} {r['real_pct']:>7.1f}%")
    print("-" * 72)
    print(f"{'TS TOTAL':32s} {total_src:>8d} {total_tested:>8d} {total_covered:>8d} {real_total:>7.1f}%")
    return 0


if __name__ == "__main__":
    sys.exit(main())

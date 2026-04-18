#!/usr/bin/env python3
"""TypeScript coverage aggregator for chorus (#2197).

Drives jest natively in each project with `--coverageReporters=json-summary`,
reads each project's `coverage/coverage-summary.json`, and aggregates the
statement counts across all projects using jest's own definition of
"covered statement" and jest's own collectCoverageFrom expansion.

Every TS project's jest.config.js already has `collectCoverageFrom` set
to include every src/**/*.ts file regardless of whether a test imports
it, so the denominator reflects real source coverage — not just the
test-visible subset. No bespoke counting.

Usage:
    coverage-ts.py [--run] [--json]

    --run   re-run jest --coverage in each project before reading
            coverage-summary.json. Without --run, uses the cached
            summary from the last jest invocation if present.
    --json  emit machine-readable JSON to stdout
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

CHORUS_ROOT = Path(os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"))

# Projects with a jest.config.js. Excludes platform/tests (cucumber-js).
PROJECTS = [
    "directing/clearing",
    "directing/products/cards",
    "platform/workflow-engine",
    "platform/chorus-sdk",
    "platform/pulse",
    "platform/api",
]


def run_jest_coverage(project_root: Path) -> bool:
    """Run jest --coverage --coverageReporters=json-summary in the project.

    Measurement success = "coverage-summary.json was produced." A non-zero
    jest exit from a coverageThreshold trip is NOT a measurement failure
    — we still got the number, and the threshold fail should inform the
    user but not block reporting. Only treat as failure if no summary
    was written (implies tests crashed or config is broken).
    """
    summary_path = project_root / "coverage" / "coverage-summary.json"
    mtime_before = summary_path.stat().st_mtime if summary_path.is_file() else 0
    try:
        result = subprocess.run(
            ["npx", "jest", "--coverage", "--silent", "--coverageReporters=json-summary"],
            cwd=project_root,
            check=False,
            capture_output=True,
            timeout=600,
        )
    except (subprocess.TimeoutExpired, OSError) as e:
        print(f"[coverage-ts] {project_root.name}: {e}", file=sys.stderr)
        return False
    # Did jest write a fresh summary? That's the measurement success criterion.
    if not summary_path.is_file() or summary_path.stat().st_mtime <= mtime_before:
        print(f"[coverage-ts] {project_root.name}: jest exit {result.returncode}, no fresh coverage-summary.json", file=sys.stderr)
        return False
    if result.returncode != 0:
        # Common case: coverageThreshold tripped. Not a measurement failure.
        print(f"[coverage-ts] {project_root.name}: jest exit {result.returncode} (threshold or warning — summary written)", file=sys.stderr)
    stamp_commit(project_root / "coverage")
    return True


def stamp_commit(cov_dir: Path) -> None:
    """Write .commit alongside coverage artifacts — HEAD SHA at time of run."""
    try:
        sha = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=CHORUS_ROOT, text=True, stderr=subprocess.DEVNULL,
        ).strip()
        cov_dir.mkdir(parents=True, exist_ok=True)
        (cov_dir / ".commit").write_text(sha + "\n")
    except (subprocess.CalledProcessError, OSError):
        pass


def cache_is_fresh(project_root: Path) -> bool:
    """Cached coverage is fresh iff .commit matches HEAD."""
    stamp = project_root / "coverage" / ".commit"
    if not stamp.is_file():
        return False
    try:
        stamped = stamp.read_text().strip()
        head = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=CHORUS_ROOT, text=True, stderr=subprocess.DEVNULL,
        ).strip()
        return stamped == head
    except (OSError, subprocess.CalledProcessError):
        return False


def read_summary(project_root: Path) -> dict | None:
    """Read coverage/coverage-summary.json → total statement counts.

    Returns None if the summary doesn't exist or is malformed.
    """
    path = project_root / "coverage" / "coverage-summary.json"
    if not path.is_file():
        return None
    try:
        with path.open("r") as fh:
            data = json.load(fh)
    except (OSError, json.JSONDecodeError):
        return None
    total = data.get("total", {}).get("statements")
    if not total:
        return None
    return {
        "covered": int(total.get("covered", 0)),
        "total": int(total.get("total", 0)),
        "pct": float(total.get("pct", 0.0)),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="store_true", help="run jest --coverage per project before reading")
    ap.add_argument("--json", action="store_true", help="machine-readable JSON output")
    args = ap.parse_args()

    rows = []
    total_stmts = 0
    total_covered = 0
    missing: list[str] = []
    failures: list[str] = []

    for proj_rel in PROJECTS:
        proj_root = CHORUS_ROOT / proj_rel
        if not (proj_root / "jest.config.js").is_file():
            continue
        if args.run:
            if not run_jest_coverage(proj_root):
                failures.append(proj_rel)
                rows.append({"project": proj_rel, "covered": 0, "total": 0, "pct": 0.0, "error": "jest run failed"})
                continue
        elif not cache_is_fresh(proj_root):
            missing.append(proj_rel)
            rows.append({"project": proj_rel, "covered": 0, "total": 0, "pct": 0.0, "error": "stale cache (commit != HEAD) — rerun with --run"})
            continue
        summary = read_summary(proj_root)
        if summary is None:
            missing.append(proj_rel)
            rows.append({"project": proj_rel, "covered": 0, "total": 0, "pct": 0.0, "error": "no coverage-summary.json"})
            continue
        total_stmts += summary["total"]
        total_covered += summary["covered"]
        rows.append({
            "project": proj_rel,
            "covered": summary["covered"],
            "total": summary["total"],
            "pct": summary["pct"],
        })

    real_total = 100.0 * total_covered / total_stmts if total_stmts else 0.0

    if args.json:
        print(json.dumps({
            "language": "ts",
            "projects": rows,
            "total": {"covered": total_covered, "total": total_stmts, "pct": round(real_total, 2)},
            "missing_summaries": missing,
        }, indent=2))
        return 0

    print(f"{'project':32s} {'covered':>10s} {'total':>10s} {'pct':>8s}")
    print("-" * 64)
    for r in rows:
        err = r.get("error", "")
        print(f"{r['project']:32s} {r['covered']:>10d} {r['total']:>10d} {r['pct']:>7.2f}%  {err}")
    print("-" * 64)
    print(f"{'TS TOTAL':32s} {total_covered:>10d} {total_stmts:>10d} {real_total:>7.2f}%")
    if missing:
        print()
        print(f"Note: {len(missing)} project(s) missing or stale — re-run with --run to refresh.")
    if failures:
        print()
        print(f"ERROR: {len(failures)} project(s) failed: {', '.join(failures)}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

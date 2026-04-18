#!/usr/bin/env python3
"""Rust coverage aggregator for chorus (#2197).

Runs `cargo llvm-cov --summary-only --json` in every Rust crate under
the chorus tree and aggregates line coverage deterministically.

Usage:
    coverage-rust.py [--run]

    --run   run cargo llvm-cov in each crate first. If omitted, reads
            the last run's target/llvm-cov artifacts when present; if
            nothing is cached, --run is forced.

Output: per-crate and total line counts plus percentage, or JSON with
--json.
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


def find_crates() -> list[Path]:
    """Find all Rust crates (Cargo.toml with [package] section, not virtual manifests).

    Excludes target/ and node_modules/ paths.
    """
    crates: list[Path] = []
    for toml in CHORUS_ROOT.rglob("Cargo.toml"):
        parts = toml.parts
        if any(skip in parts for skip in ("target", "node_modules")):
            continue
        try:
            with toml.open("r") as fh:
                content = fh.read()
        except OSError:
            continue
        # Skip virtual manifests (workspace-only, no [package])
        if "[package]" not in content:
            continue
        crates.append(toml.parent)
    return sorted(crates)


def run_llvm_cov(crate_dir: Path) -> tuple[int, int] | None:
    """Run cargo llvm-cov --summary-only --json in the crate; return (covered, total) lines.

    Returns None if the run fails.
    """
    try:
        result = subprocess.run(
            ["cargo", "llvm-cov", "--summary-only", "--json"],
            cwd=crate_dir,
            capture_output=True,
            text=True,
            timeout=600,
        )
    except (subprocess.TimeoutExpired, OSError) as e:
        print(f"[coverage-rust] {crate_dir.name}: {e}", file=sys.stderr)
        return None
    if result.returncode != 0:
        print(f"[coverage-rust] {crate_dir.name}: exit {result.returncode}\n{result.stderr[:500]}", file=sys.stderr)
        return None
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        # Fall back to parsing the text table
        return parse_text_summary(result.stdout)
    totals = data.get("data", [{}])[0].get("totals", {})
    lines = totals.get("lines", {})
    count = int(lines.get("count", 0))
    covered = int(lines.get("covered", 0))
    return covered, count


def parse_text_summary(text: str) -> tuple[int, int] | None:
    """Fallback: parse the text table llvm-cov emits if --json is unavailable."""
    for line in text.splitlines():
        if line.startswith("TOTAL"):
            parts = re.split(r"\s+", line.strip())
            # Columns: TOTAL Regions Missed Cover Functions Missed Executed Lines Missed Cover Branches Missed Cover
            try:
                total_lines = int(parts[7])
                missed_lines = int(parts[8])
                return total_lines - missed_lines, total_lines
            except (IndexError, ValueError):
                return None
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="store_true", help="force cargo llvm-cov run (default: always runs — Rust has no offline cache parallel to jest's coverage-final.json)")
    ap.add_argument("--json", action="store_true", help="machine-readable JSON output")
    args = ap.parse_args()

    crates = find_crates()
    rows = []
    total_lines = 0
    total_covered = 0

    for crate_dir in crates:
        result = run_llvm_cov(crate_dir)
        if result is None:
            rows.append({"crate": crate_dir.name, "src_lines": 0, "covered": 0, "real_pct": 0.0, "error": "run failed"})
            continue
        covered, count = result
        total_lines += count
        total_covered += covered
        real_pct = 100.0 * covered / count if count else 0.0
        rows.append({
            "crate": crate_dir.name,
            "path": str(crate_dir.relative_to(CHORUS_ROOT)),
            "src_lines": count,
            "covered": covered,
            "real_pct": round(real_pct, 2),
        })

    real_total = 100.0 * total_covered / total_lines if total_lines else 0.0

    if args.json:
        print(json.dumps({
            "language": "rust",
            "crates": rows,
            "total": {"src_lines": total_lines, "covered": total_covered, "real_pct": round(real_total, 2)},
        }, indent=2))
        return 0

    print(f"{'crate':32s} {'src':>8s} {'covered':>8s} {'real %':>8s}")
    print("-" * 64)
    for r in rows:
        print(f"{r['crate']:32s} {r['src_lines']:>8d} {r['covered']:>8d} {r['real_pct']:>7.1f}%")
    print("-" * 64)
    print(f"{'RUST TOTAL':32s} {total_lines:>8d} {total_covered:>8d} {real_total:>7.1f}%")
    return 0


if __name__ == "__main__":
    sys.exit(main())

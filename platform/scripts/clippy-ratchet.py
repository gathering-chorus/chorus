#!/usr/bin/env python3
"""
#2532: per-lint cargo clippy ratchet (mirror of platform/scripts/lint-ratchet.js).
Runs clippy on each Rust crate, compares per-lint counts against
.clippy-baseline.json. Fails if any lint count climbs above baseline.

Exit codes:
  0 — pass (every lint count at or below baseline)
  1 — ratchet violation (a lint count climbed)
  2 — lint firing that isn't in baseline (new lint added without regenerate)
  3 — clippy invocation failed

Per-crate matrix:
  chorus-inject: 0 baseline, -D warnings blocking (handled by clippy job, not here)
  chorus-hooks:  64 baseline pre-werk-init-retirement, ratchets-down-only here

Per harness disconnect plan: this ratchet is the .eslint-baseline-style sensor
Phase 1 disconnect (#2526) will retire. Build-then-retire matches the AC at
#2532 ("mirror of .eslint-baseline.json approach, retired in Phase 1 disconnect").
"""

import json
import os
import subprocess
import shutil
import sys
import time
from pathlib import Path

REPO_ROOT = Path(os.environ.get("CHORUS_ROOT") or Path(__file__).resolve().parent.parent.parent)
BASELINE_PATH = Path(os.environ.get("CLIPPY_BASELINE_PATH") or REPO_ROOT / ".clippy-baseline.json")
CRATES = [
    "platform/services/chorus-hooks",
    "platform/services/chorus-inject",
]


def collect_counts(crate: str) -> dict[str, int]:
    """Run clippy on a crate, return {lint_code: count} for clippy:: lints only."""
    crate_dir = REPO_ROOT / crate
    if not crate_dir.is_dir():
        print(f"clippy-ratchet: missing crate dir {crate_dir}", file=sys.stderr)
        sys.exit(3)
    try:
        cargo = shutil.which("cargo") or os.path.expanduser("~/.cargo/bin/cargo")  # #3187: resolve absolutely
        proc = subprocess.run(
            [cargo, "clippy", "--all-targets", "--message-format=json"],
            cwd=crate_dir,
            capture_output=True,
            text=True,
            timeout=600,
        )
    except subprocess.TimeoutExpired:
        print(f"clippy-ratchet: clippy timed out on {crate}", file=sys.stderr)
        sys.exit(3)
    counts: dict[str, int] = {}
    for line in proc.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        if obj.get("reason") != "compiler-message":
            continue
        msg = obj.get("message", {})
        code = (msg.get("code") or {}).get("code") or ""
        if not code.startswith("clippy::"):
            continue
        if msg.get("level") not in ("warning", "error"):
            continue
        counts[code] = counts.get(code, 0) + 1
    return counts


def main() -> int:
    regenerate = "--regenerate" in sys.argv

    actual: dict[str, dict[str, int]] = {}
    for crate in CRATES:
        actual[crate] = collect_counts(crate)

    if regenerate:
        BASELINE_PATH.write_text(
            json.dumps(
                {
                    "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                    "note": (
                        "Per-lint cargo clippy count ratchet (#2532). Counts may only "
                        "decrease. Run `platform/scripts/clippy-ratchet.sh --regenerate` "
                        "after a legitimate drop. Retired by Phase 1 disconnect (#2526)."
                    ),
                    "counts": actual,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n"
        )
        print(f"clippy-ratchet: regenerated {BASELINE_PATH}")
        return 0

    if not BASELINE_PATH.exists():
        print(
            f"clippy-ratchet: baseline missing at {BASELINE_PATH} — run with --regenerate first",
            file=sys.stderr,
        )
        return 3

    baseline = json.loads(BASELINE_PATH.read_text())["counts"]
    violations: list[str] = []
    new_lints: list[str] = []

    for crate, lints in actual.items():
        base = baseline.get(crate, {})
        for lint, count in lints.items():
            if lint not in base:
                new_lints.append(
                    f"{crate}: new lint {lint} ({count} hits) — regenerate baseline if intentional"
                )
            elif count > base[lint]:
                violations.append(f"{crate}: {lint} climbed {base[lint]} → {count}")

    for crate, base in baseline.items():
        actual_lints = actual.get(crate, {})
        for lint in base:
            if lint not in actual_lints:
                print(f"clippy-ratchet: {crate}: {lint} dropped to 0 — consider regenerating baseline")

    if violations:
        for v in violations:
            print(f"FAIL: {v}", file=sys.stderr)
        return 1
    if new_lints:
        for n in new_lints:
            print(f"FAIL: {n}", file=sys.stderr)
        return 2
    print("clippy-ratchet: PASS — all per-lint counts at or below baseline")
    return 0


if __name__ == "__main__":
    sys.exit(main())

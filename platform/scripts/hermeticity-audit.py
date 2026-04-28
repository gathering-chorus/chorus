#!/usr/bin/env python3
"""
Hermeticity audit scanner — #2523.

Scanner v2 (post Kade review, chat silas-kade-1777390393 2026-04-28):
  - Discovery-driven kept-set (no hardcoded paths)
  - Pytest added
  - Network rule expanded: shellouts, dynamic imports, DB clients, Rust HTTP/socket
  - Env mock-hints tightened (only explicit beforeEach/afterEach setup/restore)
  - localhost:* reclassified to "review" (in-process spawn vs daemon-hit ambiguity)
  - Bats shell-env coupling detection ($VAR / ${VAR})

Five hermeticity rules:
    1. no network    — no outbound network from test code
    2. no fs outside tmp — no file writes outside /tmp or per-test tmpdir
    3. no clock/random — no Date.now / Math.random / performance.now without mock
    4. no env coupling — no process.env reads without explicit per-test setup
    5. order-independent — verified by jest --randomize at CI (not source-grep)

Conservative — flag candidates by lexical pattern. False positives ok; false
negatives are the worse failure mode.

Usage:
    hermeticity-audit.py [--out PATH] [--root PATH]

Output: JSON to stdout (or --out path) with per-test classification.

Card #2523 wave 2.
"""

import argparse
import json
import os
import re
import sys
from pathlib import Path

# ────────────────────────── Network rule patterns ──────────────────────────

# Direct HTTP/socket calls
NETWORK_PATTERNS = [
    # JS/TS http
    re.compile(r'\bfetch\s*\('),
    re.compile(r'\baxios\b'),
    re.compile(r'\bhttp\.(get|post|put|delete|request)\s*\('),
    re.compile(r'\bhttps\.(get|post|put|delete|request)\s*\('),
    re.compile(r"require\(['\"](http|https|net|dns|tls|ws)['\"]\)"),
    re.compile(r"from ['\"](http|https|net|dns|tls|ws)['\"]"),
    # Dynamic imports (Kade #2)
    re.compile(r"await\s+import\s*\(\s*['\"](http|https|net|dns|tls|ws)['\"]"),
    re.compile(r"import\s*\(\s*['\"](http|https|net|dns|tls|ws)['\"]"),
    # Socket libs
    re.compile(r'\bsocket\.io\b'),
    re.compile(r"require\(['\"]socket\.io"),
    re.compile(r"from ['\"]socket\.io"),
    # DB clients (Kade #3) — connection = network even if Unix socket
    re.compile(r"require\(['\"](pg|ioredis|kafkajs|mongodb|mongoose|mysql|mysql2|prisma|knex|redis|@prisma/client|cassandra-driver|elasticsearch|@elastic/elasticsearch)['\"]\)"),
    re.compile(r"from ['\"](pg|ioredis|kafkajs|mongodb|mongoose|mysql|mysql2|prisma|knex|redis|@prisma/client|cassandra-driver|elasticsearch|@elastic/elasticsearch)['\"]"),
    # Shellout to network tools (Kade #1) — child_process spawn/exec invoking curl/wget/nc/ssh/scp/rsync
    re.compile(r"(spawn|exec|spawnSync|execSync)\s*\([^)]{0,80}['\"](curl|wget|nc|ssh|scp|rsync|http)\b"),
    # bats: same shellout patterns at the shell level
    re.compile(r'(?<!\w)(curl|wget|nc)\b'),  # may over-flag; downgraded for non-bats below
    # Rust HTTP/socket (Kade #4)
    re.compile(r"\breqwest::"),
    re.compile(r"\bureq::"),
    re.compile(r"\bhyper::(Client|client)"),
    re.compile(r"\bsurf::"),
    re.compile(r"\bisahc::"),
    re.compile(r"\bstd::net::(TcpStream|TcpListener|UdpSocket)"),
    re.compile(r"\btokio::net::(TcpStream|TcpListener|UdpSocket|UnixStream)"),
    re.compile(r"\basync_std::net::"),
    # Python network
    re.compile(r"\brequests\.(get|post|put|delete|request)\s*\("),
    re.compile(r"\bhttpx\.(get|post|put|delete|request|Client|AsyncClient)"),
    re.compile(r"\baiohttp\."),
    re.compile(r"\burllib\.request\."),
    re.compile(r"\bhttp\.client\."),
    re.compile(r"^\s*import\s+(requests|httpx|aiohttp|urllib|http\.client|socket)\b", re.MULTILINE),
    re.compile(r"^\s*from\s+(requests|httpx|aiohttp|urllib|http\.client|socket)\s+import", re.MULTILINE),
]

# localhost / 127.0.0.1 — moved to "review" not "fail" (Kade #6) because
# in-process spawned servers ARE hermetic but match this pattern. Manual review
# distinguishes "I spawned this server" (hermetic) from "I'm hitting a daemon"
# (non-hermetic).
LOCALHOST_PATTERNS = [
    re.compile(r"localhost:\d+"),
    re.compile(r"127\.0\.0\.1:\d+"),
    re.compile(r"http://localhost"),
    re.compile(r"http://127\.0\.0\.1"),
]

# ────────────────────────── FS rule patterns ──────────────────────────

FS_WRITE_PATTERNS = [
    re.compile(r'fs\.(writeFileSync|writeFile|mkdirSync|mkdir|rmSync|rm|unlinkSync|unlink|appendFileSync|appendFile)'),
    re.compile(r"std::fs::(write|create_dir|remove_file|remove_dir|create_dir_all|remove_dir_all)"),
    re.compile(r'tokio::fs::(write|create_dir|remove_file|remove_dir|create_dir_all|remove_dir_all)'),
    re.compile(r"^\s*(open|with\s+open)\s*\([^)]*['\"]w['\"]", re.MULTILINE),  # python open(..., 'w')
    re.compile(r"\bos\.(remove|unlink|mkdir|makedirs|rmdir)\s*\("),
    re.compile(r"\bshutil\.(rmtree|copy|move)\s*\("),
    re.compile(r"\bpathlib\.Path\([^)]*\)\.write_(text|bytes)"),
]
FS_TMP_HINTS = [
    re.compile(r'/tmp/'),
    re.compile(r'\bos\.tmpdir\(\)'),
    re.compile(r'\btempfile::|\btempdir\b'),
    re.compile(r'\btmpDir\b|\btmp_dir\b'),
    re.compile(r"\btempfile\."),  # python
    re.compile(r"\bTemporaryDirectory\("),
]

# ────────────────────────── Clock/random patterns ──────────────────────────

CLOCK_RANDOM_PATTERNS = [
    re.compile(r'\bDate\.now\s*\('),
    re.compile(r'\bnew\s+Date\s*\(\s*\)'),
    re.compile(r'\bMath\.random\s*\('),
    re.compile(r'\bperformance\.now\s*\('),
    re.compile(r'\bcrypto\.randomUUID\s*\('),
    re.compile(r'\bcrypto\.randomBytes\s*\('),
    re.compile(r"std::time::SystemTime::now"),
    re.compile(r"chrono::(Utc|Local)::now"),
    re.compile(r"\brand::"),
    # Python
    re.compile(r"\btime\.time\s*\("),
    re.compile(r"\btime\.monotonic\s*\("),
    re.compile(r"\bdatetime\.datetime\.now\s*\("),
    re.compile(r"\brandom\.(random|randint|choice|uniform|shuffle)\s*\("),
    re.compile(r"\buuid\.uuid[14]\s*\("),
]
CLOCK_MOCK_HINTS = [
    re.compile(r'\bjest\.useFakeTimers'),
    re.compile(r'\bjest\.setSystemTime'),
    re.compile(r'\bsinon\.useFakeTimers'),
    re.compile(r'\bfreezegun\b'),  # python
    re.compile(r"@freeze_time"),
]

# ────────────────────────── Env rule patterns ──────────────────────────

ENV_PATTERNS = [
    # Reads only — process.env.X NOT followed by `=` (assignment).
    # Kade's #2524 review: assignment IS the mock; flagging both reads and
    # writes false-flags unit tests that mock env via assignment.
    re.compile(r'\bprocess\.env\.\w+(?!\s*=)'),
    re.compile(r"\bprocess\.env\[['\"][^'\"]+['\"]\](?!\s*=)"),
    re.compile(r'\benv::var\s*\('),
    re.compile(r'\bstd::env::var'),
    # Python
    re.compile(r"\bos\.environ\b"),
    re.compile(r"\bos\.getenv\s*\("),
]
# Tightened (Kade #5): only explicit beforeEach/afterEach setting+restoring env
# counts as a mock hint. Removed the CHORUS_INJECT_DRY_RUN/CHORUS_ROOT
# substring downgrade — mentioning a var doesn't prove it's mocked.
ENV_MOCK_HINTS = [
    re.compile(r"beforeEach\s*\([^)]*\)\s*=>\s*\{[^}]*process\.env", re.DOTALL),
    re.compile(r"afterEach\s*\([^)]*\)\s*=>\s*\{[^}]*delete\s+process\.env", re.DOTALL),
    re.compile(r"const\s+OLD_ENV\s*="),  # canonical jest env-restore pattern
    # Direct assignment IS the mock — process.env.X = ... at any scope.
    # Kade #2524 review: unit tests that wire env via assignment should be
    # treated as hermetic, not flagged as env-coupled reads.
    re.compile(r"process\.env\.\w+\s*="),
    re.compile(r"process\.env\[['\"][^'\"]+['\"]\]\s*="),
    re.compile(r"@pytest\.fixture[^)]*monkeypatch"),
    re.compile(r"\bmonkeypatch\.setenv"),
    re.compile(r"\bmonkeypatch\.delenv"),
]

# Bats shell-env (Kade bonus). Detects $VAR or ${VAR} reads in .bats.
BATS_SHELL_ENV_PATTERN = re.compile(r"\$\{?([A-Z_][A-Z_0-9]*)\}?")
# Bats env mock hints — setup() / teardown() functions exporting/unsetting vars
BATS_ENV_MOCK_HINTS = [
    re.compile(r"^\s*setup\s*\(\s*\)\s*\{", re.MULTILINE),
    re.compile(r"^\s*teardown\s*\(\s*\)\s*\{", re.MULTILINE),
    re.compile(r"\bexport\s+[A-Z_]+="),
    re.compile(r"\bunset\s+[A-Z_]+\b"),
]


def classify_file(path: Path, kind: str) -> dict:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        return {"path": str(path), "error": str(e)}

    rules = {}

    # Rule 1 — network
    is_bats = kind == "bats"
    network_hits = []
    for p in NETWORK_PATTERNS:
        # The bare curl/wget/nc word pattern only applies to bats
        if p.pattern == r'(?<!\w)(curl|wget|nc)\b' and not is_bats:
            continue
        for m in p.finditer(text):
            network_hits.append(m.group(0))
    network_hits = list(dict.fromkeys(network_hits))[:8]

    # localhost — separate "review" status (not pure fail)
    localhost_hits = list(dict.fromkeys(
        m.group(0) for p in LOCALHOST_PATTERNS for m in p.finditer(text)
    ))[:5]

    if network_hits:
        rules["network"] = {"status": "fail", "hits": network_hits}
    elif localhost_hits:
        rules["network"] = {
            "status": "maybe",
            "hits": localhost_hits,
            "note": "localhost hits — distinguish in-process spawn (hermetic) vs daemon hit (non-hermetic) by manual review",
        }
    else:
        rules["network"] = {"status": "pass", "hits": []}

    # Rule 2 — fs
    fs_hits = list(dict.fromkeys(
        m.group(0) for p in FS_WRITE_PATTERNS for m in p.finditer(text)
    ))[:5]
    has_tmp_hint = any(p.search(text) for p in FS_TMP_HINTS)
    if not fs_hits:
        rules["fs"] = {"status": "pass", "hits": []}
    elif has_tmp_hint:
        rules["fs"] = {"status": "maybe", "hits": fs_hits, "note": "writes present + tmp hint — verify all writes target tmp"}
    else:
        rules["fs"] = {"status": "fail", "hits": fs_hits}

    # Rule 3 — clock/random
    clock_hits = list(dict.fromkeys(
        m.group(0) for p in CLOCK_RANDOM_PATTERNS for m in p.finditer(text)
    ))[:5]
    has_clock_mock = any(p.search(text) for p in CLOCK_MOCK_HINTS)
    if not clock_hits:
        rules["clock"] = {"status": "pass", "hits": []}
    elif has_clock_mock:
        rules["clock"] = {"status": "maybe", "hits": clock_hits, "note": "clock/random + mock hint — verify mock covers all"}
    else:
        rules["clock"] = {"status": "fail", "hits": clock_hits}

    # Rule 4 — env
    if is_bats:
        # Shell env coupling detection
        env_vars = list(dict.fromkeys(
            m.group(1) for m in BATS_SHELL_ENV_PATTERN.finditer(text)
            if m.group(1) not in ("BATS_TEST_DIRNAME", "BATS_TEST_FILENAME", "BATS_TMPDIR", "BATS_TEST_NAME", "BATS_RUN_TMPDIR", "PATH", "HOME", "USER", "PWD", "1", "2", "3")
        ))[:8]
        has_bats_mock = any(p.search(text) for p in BATS_ENV_MOCK_HINTS)
        if not env_vars:
            rules["env"] = {"status": "pass", "hits": []}
        elif has_bats_mock:
            rules["env"] = {"status": "maybe", "hits": [f"${v}" for v in env_vars], "note": "shell env reads + setup/teardown — verify all are explicitly set"}
        else:
            rules["env"] = {"status": "fail", "hits": [f"${v}" for v in env_vars]}
    else:
        env_hits = list(dict.fromkeys(
            m.group(0) for p in ENV_PATTERNS for m in p.finditer(text)
        ))[:5]
        has_env_mock = any(p.search(text) for p in ENV_MOCK_HINTS)
        if not env_hits:
            rules["env"] = {"status": "pass", "hits": []}
        elif has_env_mock:
            rules["env"] = {"status": "maybe", "hits": env_hits, "note": "env reads + explicit beforeEach/monkeypatch — verify all are mocked"}
        else:
            rules["env"] = {"status": "fail", "hits": env_hits}

    # Rule 5 — deferred to CI
    rules["order"] = {"status": "deferred", "note": "verified by jest --randomize at CI (#2532)"}

    statuses = [r["status"] for k, r in rules.items() if k != "order"]
    if all(s == "pass" for s in statuses):
        verdict = "hermetic"
    elif "fail" in statuses:
        verdict = "non-hermetic"
    else:
        verdict = "review"

    # Suggested remediation per AC3 — heuristic mapping. Manual review can override.
    remediation = suggest_remediation(rules, kind, path, text)

    return {"path": str(path), "verdict": verdict, "rules": rules, "remediation": remediation}


SPAWN_HINT_PATTERNS = [
    re.compile(r"\bspawn\s*\("),
    re.compile(r"\bfork\s*\("),
    re.compile(r"\bspawnSync\s*\("),
    re.compile(r"\bexec(?:Sync)?\s*\(\s*['\"]node\b"),
    re.compile(r"\bcreateServer\s*\("),
    re.compile(r"\.listen\s*\("),
    re.compile(r"http\.createServer"),
    # Rust spawn patterns
    re.compile(r"std::process::Command::"),
    re.compile(r"tokio::spawn\b"),
]


def suggest_remediation(rules: dict, kind: str, path: Path, text: str = "") -> dict:
    """Map rule violations to one of three buckets per AC3:
       - fix      — fix in place; mock the dependency
       - rename   — move to *.integration.test.ts (TS) or document as integration (bats/cargo/pytest)
       - review   — manual confirm (e.g. spawn-its-own-server case Kade flagged)
    Heuristic. Manual review can override; the suggested column gives every test
    a default plan so AC3 closes without 220 hand-classifications.
    """
    if all(r.get("status") == "pass" for k, r in rules.items() if k != "order"):
        return {"bucket": "none", "reason": "hermetic — no remediation needed"}

    fail_rules = [k for k, v in rules.items() if v.get("status") == "fail"]
    maybe_rules = [k for k, v in rules.items() if v.get("status") == "maybe"]

    # Network shellouts to curl/wget/nc/ssh = real network = rename
    network_info = rules.get("network", {})
    network_hits = network_info.get("hits", [])
    has_real_network = any(
        any(tok in h.lower() for tok in ("fetch", "axios", "http.", "https.", "reqwest", "ureq", "hyper", "tcp", "requests.", "httpx", "aiohttp", "urllib", "socket", "ws", "pg", "ioredis", "kafkajs", "mongodb", "mongoose", "mysql", "redis", "prisma"))
        for h in network_hits
    )
    has_shellout_curl = any(
        ("curl" in h or "wget" in h or "ssh" in h or "scp" in h or "rsync" in h or "(nc" in h or " nc " in h)
        for h in network_hits
    )
    has_localhost_only = (
        not network_hits
        and rules.get("network", {}).get("status") == "maybe"
        and rules.get("network", {}).get("hits")
    )

    # Spawn-detection (Kade's #2523 wave 3 review): if the file imports http
    # AND contains a spawn/createServer/listen pattern, the network hits may
    # be self-spawned ephemeral servers — downgrade rename to review.
    has_spawn_hint = any(p.search(text) for p in SPAWN_HINT_PATTERNS) if text else False

    # Multi-rule failures or real network/shellouts → rename to integration
    if has_real_network or has_shellout_curl:
        if has_spawn_hint and not has_shellout_curl:
            return {"bucket": "review", "reason": "network access + spawn pattern — likely in-process ephemeral server (hermetic), needs manual confirm"}
        return {"bucket": "rename", "reason": "real network / shellout — belongs in integration tier"}

    if len(fail_rules) >= 2:
        return {"bucket": "rename", "reason": f"multiple rule fails ({', '.join(fail_rules)}) — easier to integration-tier than mock"}

    # Single rule fail — usually a fix-in-place
    if fail_rules == ["clock"]:
        return {"bucket": "fix", "reason": "add jest.useFakeTimers / setSystemTime, or freezegun for pytest"}
    if fail_rules == ["env"]:
        return {"bucket": "fix", "reason": "add explicit beforeEach/afterEach env setup+restore (or pytest monkeypatch)"}
    if fail_rules == ["fs"]:
        return {"bucket": "fix", "reason": "switch fs writes to /tmp via os.tmpdir() / tempfile / TempDir"}
    if fail_rules == ["network"] and has_localhost_only:
        return {"bucket": "review", "reason": "localhost only — distinguish in-process spawn (hermetic, mark as such) vs daemon-hit (rename to integration)"}
    if fail_rules == ["network"]:
        return {"bucket": "rename", "reason": "network access without spawn pattern — integration tier"}

    # Maybe-only (review verdict) — flag for manual confirmation
    if maybe_rules:
        return {"bucket": "review", "reason": f"maybe-flags ({', '.join(maybe_rules)}) — manual confirm hermetic-as-claimed or rename"}

    return {"bucket": "review", "reason": "uncategorized"}


# ────────────────────────── Discovery-driven kept-set ──────────────────────────

EXCLUDE_RE = re.compile(r"/(node_modules|dist|target|coverage|\.git|\.next|build)/")


def discover_jest_packages(root: Path) -> list:
    """Find all directories with a package.json that has a jest config (either
    inline or jest.config.* sibling). Returns list of package roots."""
    pkgs = []
    for pj in root.rglob("package.json"):
        if EXCLUDE_RE.search(str(pj)):
            continue
        try:
            data = json.loads(pj.read_text(encoding="utf-8", errors="replace"))
        except (OSError, json.JSONDecodeError):
            continue
        has_jest = "jest" in data or any(
            (pj.parent / f).exists()
            for f in ("jest.config.js", "jest.config.ts", "jest.config.cjs", "jest.config.mjs", "jest.config.json")
        )
        # Also consider scripts that invoke jest
        scripts = data.get("scripts", {}) if isinstance(data.get("scripts"), dict) else {}
        invokes_jest = any("jest" in str(v) for v in scripts.values())
        if has_jest or invokes_jest:
            pkgs.append(pj.parent)
    return pkgs


def discover_cargo_crates(root: Path) -> list:
    """Find all directories with Cargo.toml under platform/ or anywhere with
    [package] section. Returns crate roots."""
    crates = []
    for ct in root.rglob("Cargo.toml"):
        if EXCLUDE_RE.search(str(ct)):
            continue
        try:
            text = ct.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "[package]" in text or "[workspace]" in text:
            # workspace alone is not a crate; need [package]
            if "[package]" in text:
                crates.append(ct.parent)
    return crates


def discover_pytest_roots(root: Path) -> list:
    """Find directories that look like pytest test homes: pyproject.toml with
    [tool.pytest.ini_options], conftest.py, or directories containing test_*.py
    / *_test.py files."""
    roots = set()
    # pyproject.toml with pytest config
    for pp in root.rglob("pyproject.toml"):
        if EXCLUDE_RE.search(str(pp)):
            continue
        try:
            text = pp.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if "[tool.pytest" in text:
            roots.add(pp.parent)
    # conftest.py
    for cf in root.rglob("conftest.py"):
        if EXCLUDE_RE.search(str(cf)):
            continue
        roots.add(cf.parent)
    # test_*.py / *_test.py files — collect their parent dirs
    for p in root.rglob("test_*.py"):
        if EXCLUDE_RE.search(str(p)):
            continue
        roots.add(p.parent)
    for p in root.rglob("*_test.py"):
        if EXCLUDE_RE.search(str(p)):
            continue
        roots.add(p.parent)
    return list(roots)


def kept_set(root: Path) -> list:
    """Discover the kept-set across jest, bats, cargo, pytest. Returns list of
    (kind, path) tuples."""
    out = []

    # jest TS/JS — *.test.ts / *.test.js (not *.{integration,contract,smoke}.test.*)
    integration_suffixes = (".integration.test.ts", ".contract.test.ts", ".smoke.test.ts",
                            ".integration.test.js", ".contract.test.js", ".smoke.test.js")
    for pkg in discover_jest_packages(root):
        for ext in ("*.test.ts", "*.test.js"):
            for f in pkg.rglob(ext):
                s = str(f)
                if EXCLUDE_RE.search(s):
                    continue
                if any(f.name.endswith(suf) for suf in integration_suffixes):
                    continue
                out.append(("jest", f))

    # bats — anywhere except excluded
    for f in root.rglob("*.bats"):
        if EXCLUDE_RE.search(str(f)):
            continue
        out.append(("bats", f))

    # cargo
    for crate in discover_cargo_crates(root):
        # external tests
        for f in crate.rglob("tests/**/*.rs") if (crate / "tests").exists() else []:
            if EXCLUDE_RE.search(str(f)):
                continue
            out.append(("cargo-external", f))
        # inline tests
        for f in (crate / "src").rglob("*.rs") if (crate / "src").exists() else []:
            if EXCLUDE_RE.search(str(f)):
                continue
            try:
                text = f.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if "#[test]" in text or "#[cfg(test)]" in text:
                out.append(("cargo-inline", f))

    # pytest
    seen_py = set()
    for proot in discover_pytest_roots(root):
        for pat in ("test_*.py", "*_test.py"):
            for f in proot.rglob(pat):
                if EXCLUDE_RE.search(str(f)):
                    continue
                if f in seen_py:
                    continue
                seen_py.add(f)
                out.append(("pytest", f))

    # Dedup by path (jest discovery may overlap when nested packages exist)
    seen = set()
    deduped = []
    for kind, f in out:
        if f in seen:
            continue
        seen.add(f)
        deduped.append((kind, f))
    return deduped


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", help="output path; default stdout")
    parser.add_argument("--root", default=os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus"))
    args = parser.parse_args()

    root = Path(args.root)
    tests = kept_set(root)

    results = []
    for kind, f in tests:
        rec = classify_file(f, kind)
        rec["kind"] = kind
        rec["path"] = str(f.relative_to(root))
        results.append(rec)

    summary = {
        "scanner_version": "v2",
        "total": len(results),
        "by_verdict": {},
        "by_kind": {},
    }
    for r in results:
        v = r.get("verdict", "error")
        summary["by_verdict"][v] = summary["by_verdict"].get(v, 0) + 1
        k = r.get("kind", "?")
        summary["by_kind"][k] = summary["by_kind"].get(k, 0) + 1

    payload = {
        "schema_version": "2",
        "card": "2523",
        "generated_at": __import__("datetime").datetime.now().isoformat(),
        "summary": summary,
        "results": results,
    }
    out_str = json.dumps(payload, indent=2)
    if args.out:
        Path(args.out).write_text(out_str)
        print(f"wrote {len(results)} test records to {args.out}", file=sys.stderr)
        print(json.dumps(summary, indent=2))
    else:
        print(out_str)


if __name__ == "__main__":
    main()

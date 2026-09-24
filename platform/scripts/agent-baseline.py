#!/usr/bin/env python3
"""Read-only, secret-free inventory for an operator's agent migration baseline.

Outputs presence, versions, hashes and configuration key names. Never prints
tokens, environment values, prompts, transcript contents, or hook arguments.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def config_file(path):
    record = {"path": str(path), "present": path.is_file()}
    if not record["present"]:
        return record
    record["sha256"] = digest(path)
    try:
        data = json.loads(path.read_text())
        record["keys"] = sorted(data) if isinstance(data, dict) else []
        record["hook_events"] = sorted(data.get("hooks", {}))
        record["mcp_server_names"] = sorted(data.get("mcpServers", {}))
        record["profile_names"] = sorted(data.get("profiles", {}))
        record["environment_names"] = sorted(data.get("env", {}))
    except (ValueError, TypeError, AttributeError):
        record["parsed"] = False
    return record


def baseline(home, chorus_home, probe=True):
    home, chorus_home = Path(home), Path(chorus_home)
    result = {"version": 1, "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "scope": "this OS user only; not a remote team deployment", "runtimes": {},
              "settings": [], "launch_agents": [], "instructions": [], "binaries": [],
              "session_registry": {"v2_count": 0, "legacy_present": False}}
    for runtime in ("claude", "codex", "opencode", "gemini"):
        executable = shutil.which(runtime)
        record = {"executable": executable, "installed_on_path": bool(executable), "authentication": "not_inspected"}
        if executable and probe:
            try:
                run = subprocess.run([executable, "--version"], capture_output=True, timeout=5, text=True, check=False)
                record["version"] = run.stdout.strip()[:512] if run.returncode == 0 else "probe_failed"
            except (OSError, subprocess.TimeoutExpired):
                record["version"] = "probe_unavailable"
        result["runtimes"][runtime] = record
    for relative in (".claude/settings.json", ".Codex/settings.json", ".codex/hooks.json",
                     ".codex/config.toml", ".gemini/settings.json", ".config/opencode/opencode.json",
                     ".chorus/agent-profiles.json"):
        result["settings"].append(config_file(home / relative))
    for role in ("wren", "silas", "kade"):
        for name in ("CLAUDE.md", "AGENTS.md", "GEMINI.md"):
            source = chorus_home / "roles" / role / name
            if source.is_file():
                result["instructions"].append({"role": role, "file": str(source), "sha256": digest(source), "bytes": source.stat().st_size})
    for source in sorted((home / "Library/LaunchAgents").glob("*chorus*.plist")):
        try:
            data = plistlib.loads(source.read_bytes())
            result["launch_agents"].append({"file": str(source), "sha256": digest(source), "label": data.get("Label"),
                "program": data.get("Program") or (data.get("ProgramArguments") or [None])[0],
                "environment_names": sorted(data.get("EnvironmentVariables", {}))})
        except (ValueError, OSError, plistlib.InvalidFileException):
            result["launch_agents"].append({"file": str(source), "parsed": False})
    for name in ("chorus-awake", "chorus-hooks", "chorus-hook-shim", "chorus-inject", "chorus-agent", "chorus-agentd"):
        source = home / ".chorus/bin" / name
        result["binaries"].append({"name": name, "present": source.is_file(), "sha256": digest(source) if source.is_file() else None})
    registry = home / ".chorus/sessions"
    result["session_registry"] = {"v2_count": len(list((registry / "v2").glob("*.json"))), "legacy_present": registry.exists()}
    result["environment_names"] = sorted(name for name in os.environ if name.startswith(("CHORUS_", "CLAUDE_", "CODEX_", "GEMINI_")))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--home", type=Path, default=Path.home())
    parser.add_argument("--chorus-home", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--no-probe", action="store_true")
    args = parser.parse_args()
    print(json.dumps(baseline(args.home, args.chorus_home, not args.no_probe), indent=2))

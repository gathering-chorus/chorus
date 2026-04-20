import json, sys, os, subprocess, tempfile, re, hashlib
from datetime import datetime

manifest_path = sys.argv[1]
claudemd_dir = sys.argv[2]
mode = sys.argv[3]
role_filter = sys.argv[4] if len(sys.argv) > 4 and sys.argv[4] else ""
card_ref = sys.argv[5] if len(sys.argv) > 5 and sys.argv[5] else ""

# Load manifest
with open(manifest_path) as f:
    manifest = json.load(f)

# --- Pre-flight validation (runs on every mode) ---
def validate_manifest(manifest, claudemd_dir, verbose=False):
    """Validate manifest structure, fragment integrity, and variable coverage."""
    errors = []
    warnings = []

    # 1. Version is a monotonic integer (or legacy semver for history)
    version = manifest.get("version", "")
    if not re.match(r'^\d+$', version) and not re.match(r'^\d+\.\d+\.\d+$', version):
        errors.append(f"Version '{version}' is not valid (expected integer N or legacy N.N.N)")

    # 2. Required top-level keys
    for key in ("version", "variables", "roles"):
        if key not in manifest:
            errors.append(f"Missing required key: '{key}'")

    if "variables" not in manifest or "roles" not in manifest:
        return errors, warnings  # Can't continue without these

    variables = manifest["variables"]
    roles = manifest["roles"]

    # 3. Every role in 'roles' has a matching entry in 'variables'
    for role_name in roles:
        if role_name not in variables:
            errors.append(f"Role '{role_name}' in 'roles' has no entry in 'variables'")

    # 4. Every entry in 'variables' has a matching role in 'roles'
    for var_name in variables:
        if var_name not in roles:
            warnings.append(f"Variables defined for '{var_name}' but no matching role in 'roles'")

    # 5. Each role has 'output' and 'sections'
    for role_name, role_config in roles.items():
        if "output" not in role_config:
            errors.append(f"Role '{role_name}' missing 'output' path")
        if "sections" not in role_config:
            errors.append(f"Role '{role_name}' missing 'sections' list")
            continue

        sections = role_config["sections"]
        if not sections:
            errors.append(f"Role '{role_name}' has empty sections list")

        # 6. Check for duplicate sections
        seen = set()
        for s in sections:
            if s in seen:
                errors.append(f"Role '{role_name}' has duplicate section: {s}")
            seen.add(s)

        # 7. All fragment files exist
        for section_path in sections:
            full_path = os.path.join(claudemd_dir, section_path)
            if not os.path.exists(full_path):
                errors.append(f"Role '{role_name}': fragment not found: {section_path}")

        # 8. Variable coverage — check that all {{VAR}} placeholders in fragments
        #    have definitions in this role's variables
        if role_name in variables:
            role_vars = variables[role_name]
            for section_path in sections:
                full_path = os.path.join(claudemd_dir, section_path)
                if not os.path.exists(full_path):
                    continue
                with open(full_path) as f:
                    content = f.read()
                placeholders = set(re.findall(r'\{\{([A-Z_]+)\}\}', content))
                # Variables injected by the generator (not required in manifest.variables)
                injected = {"CHORUS_PROMPT_VERSION"}
                for ph in placeholders:
                    if ph not in role_vars and ph not in injected:
                        errors.append(f"Role '{role_name}': {{{{{ph}}}}} in {section_path} has no variable definition")

        # 9. Output directory exists
        if "output" in role_config:
            messages_dir = claudemd_dir  # #2150: paths are relative to claudemd_dir, not its parent
            output_path = os.path.normpath(os.path.join(messages_dir, role_config["output"]))
            output_dir = os.path.dirname(output_path)
            if not os.path.exists(output_dir):
                errors.append(f"Role '{role_name}': output directory does not exist: {output_dir}")

    if verbose:
        # Print all checks that passed
        ver_ok = "OK" if (re.match(r'^\d+$', version) or re.match(r'^\d+\.\d+\.\d+$', version)) else "INVALID"
        print(f"  Version: {version} ({ver_ok})")
        print(f"  Roles: {', '.join(roles.keys())}")
        print(f"  Variables: {', '.join(variables.keys())}")
        total_fragments = sum(len(r.get('sections', [])) for r in roles.values())
        shared = set()
        for r in roles.values():
            for s in r.get('sections', []):
                if s.startswith('shared/'):
                    shared.add(s)
        print(f"  Fragments: {total_fragments} total ({len(shared)} shared)")

    return errors, warnings

# Always validate
validation_errors, validation_warnings = validate_manifest(manifest, claudemd_dir, verbose=(mode == "validate"))

if mode == "validate":
    for w in validation_warnings:
        print(f"  WARN: {w}", file=sys.stderr)
    for e in validation_errors:
        print(f"  FAIL: {e}", file=sys.stderr)

    if validation_errors:
        print(f"\nValidation: {len(validation_errors)} error(s), {len(validation_warnings)} warning(s)")
        sys.exit(1)
    else:
        print(f"\nValidation: PASS ({len(validation_warnings)} warning(s))")
        sys.exit(0)

# For other modes, fail on errors, warn on warnings
for w in validation_warnings:
    print(f"  WARN: {w}", file=sys.stderr)
if validation_errors:
    for e in validation_errors:
        print(f"  FAIL: {e}", file=sys.stderr)
    print(f"\nManifest validation failed ({len(validation_errors)} error(s)). Fix manifest.json before generating.", file=sys.stderr)
    sys.exit(1)

version = manifest["version"]
variables = manifest["variables"]
roles = manifest["roles"]
reference = manifest.get("reference", None)
size_budget = manifest.get("size_budget", 40000)
now = datetime.now().strftime("%Y-%m-%d %H:%M")

# --- Checksum tracking for drift detection ---
CHECKSUMS_PATH = os.path.join(claudemd_dir, ".checksums.json")

def hash_file(path):
    """SHA256 hash of a file's contents."""
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        h.update(f.read())
    return h.hexdigest()

def compute_input_hash(role_name, role_config, variables, manifest_path, claudemd_dir):
    """Hash all inputs that affect a role's generated output.
    Includes fragments, variables, and permission profiles (settings.local.json)."""
    h = hashlib.sha256()
    # Include manifest structure (not just content — variable values matter)
    h.update(json.dumps(role_config, sort_keys=True).encode())
    h.update(json.dumps(variables.get(role_name, {}), sort_keys=True).encode())
    # Include each fragment's content
    for section_path in role_config.get("sections", []):
        full_path = os.path.join(claudemd_dir, section_path)
        if os.path.exists(full_path):
            with open(full_path, 'rb') as f:
                h.update(f.read())
    # Include permission profile (settings.local.json) — part of Werk
    repo_root = os.path.dirname(os.path.dirname(claudemd_dir))  # messages/ -> gathering-team/
    settings_path = os.path.join(repo_root, "roles", role_name,
                                 ".claude", "settings.local.json")
    if os.path.exists(settings_path):
        with open(settings_path, 'rb') as f:
            h.update(f.read())
    return h.hexdigest()

def compute_fragment_hashes(role_name, role_config, claudemd_dir):
    """Hash each fragment individually for change reporting."""
    hashes = {}
    for section_path in role_config.get("sections", []):
        full_path = os.path.join(claudemd_dir, section_path)
        if os.path.exists(full_path):
            hashes[section_path] = hash_file(full_path)
    return hashes

def load_checksums():
    """Load stored checksums from last generation."""
    if os.path.exists(CHECKSUMS_PATH):
        with open(CHECKSUMS_PATH) as f:
            return json.load(f)
    return {}

def save_checksums(checksums):
    """Write checksums after successful generation."""
    with open(CHECKSUMS_PATH, 'w') as f:
        json.dump(checksums, f, indent=2)
        f.write("\n")

def find_changed_fragments(role_name, role_config, claudemd_dir, stored):
    """Compare per-fragment hashes to find which files changed."""
    current = compute_fragment_hashes(role_name, role_config, claudemd_dir)
    stored_frags = stored.get(role_name, {}).get("fragments", {})
    changed = []
    for path, current_hash in current.items():
        if path not in stored_frags:
            changed.append(f"{path} (new)")
        elif stored_frags[path] != current_hash:
            changed.append(path)
    # Check for removed fragments
    for path in stored_frags:
        if path not in current:
            changed.append(f"{path} (removed)")
    return changed

# --- Version snapshots for compatibility matrix ---
VERSIONS_DIR = os.path.join(claudemd_dir, "versions")

def save_version_snapshot(version, roles, variables, claudemd_dir):
    """Archive fragment hashes for a version. Creates versions/ dir if needed."""
    os.makedirs(VERSIONS_DIR, exist_ok=True)
    snapshot = {
        "version": version,
        "archived_at": datetime.now().strftime("%Y-%m-%d %H:%M"),
        "roles": {}
    }
    for role_name, role_config in roles.items():
        snapshot["roles"][role_name] = {
            "sections": role_config.get("sections", []),
            "fragments": compute_fragment_hashes(role_name, role_config, claudemd_dir),
            "var_hash": hashlib.sha256(json.dumps(variables.get(role_name, {}), sort_keys=True).encode()).hexdigest(),
        }
    path = os.path.join(VERSIONS_DIR, f"{version}.json")
    with open(path, 'w') as f:
        json.dump(snapshot, f, indent=2)
        f.write("\n")
    return path

def load_version_snapshot(version):
    """Load a version snapshot."""
    path = os.path.join(VERSIONS_DIR, f"{version}.json")
    if os.path.exists(path):
        with open(path) as f:
            return json.load(f)
    return None

def list_version_snapshots():
    """List all archived versions, sorted."""
    if not os.path.exists(VERSIONS_DIR):
        return []
    versions = []
    for f in sorted(os.listdir(VERSIONS_DIR)):
        if f.endswith('.json'):
            versions.append(f.replace('.json', ''))
    return versions

def read_output_version(output_path):
    """Read the version from a generated CLAUDE.md header."""
    if not os.path.exists(output_path):
        return None
    with open(output_path) as f:
        first_line = f.readline()
    # Header: <!-- GENERATED by claudemd-gen.sh | v1.3.0 | 2026-02-21 11:55 | DO NOT EDIT DIRECTLY -->
    match = re.search(r'v(\d+\.\d+\.\d+)', first_line)
    return match.group(1) if match else None

def diff_versions(v1_snap, v2_snap):
    """Diff two version snapshots. Returns per-role changes."""
    changes = {}
    all_roles = set(list(v1_snap.get("roles", {}).keys()) + list(v2_snap.get("roles", {}).keys()))
    for role in sorted(all_roles):
        r1 = v1_snap.get("roles", {}).get(role, {})
        r2 = v2_snap.get("roles", {}).get(role, {})
        f1 = r1.get("fragments", {})
        f2 = r2.get("fragments", {})
        role_changes = []
        # Added fragments
        for path in sorted(set(f2.keys()) - set(f1.keys())):
            role_changes.append(f"  + {path}")
        # Removed fragments
        for path in sorted(set(f1.keys()) - set(f2.keys())):
            role_changes.append(f"  - {path}")
        # Modified fragments
        for path in sorted(set(f1.keys()) & set(f2.keys())):
            if f1[path] != f2[path]:
                role_changes.append(f"  ~ {path}")
        # Variable changes
        if r1.get("var_hash") != r2.get("var_hash"):
            role_changes.append(f"  ~ (variables changed)")
        if role_changes:
            changes[role] = role_changes
    return changes

# Filter roles if specified
if role_filter:
    if role_filter not in roles:
        print(f"ERROR: Unknown role '{role_filter}'. Valid: {', '.join(roles.keys())}", file=sys.stderr)
        sys.exit(1)
    roles = {role_filter: roles[role_filter]}

header_comment = f"<!-- GENERATED by claudemd-gen.sh | v{version} | {now} | DO NOT EDIT DIRECTLY -->"

def resolve_output_path(output_rel):
    """Resolve output path relative to claudemd_dir (designing/claudemd/)."""
    return os.path.normpath(os.path.join(claudemd_dir, output_rel))

def read_fragment(section_path):
    """Read a fragment file from claudemd_dir"""
    full_path = os.path.join(claudemd_dir, section_path)
    if not os.path.exists(full_path):
        print(f"  ERROR: Fragment not found: {section_path}", file=sys.stderr)
        sys.exit(1)
    with open(full_path) as f:
        return f.read()

# --- #2311: protocol contract (chorus-prompt/X.Y + core hash + role-fragments hash) ---

PROTOCOL_VERSION_PATH = os.path.join(claudemd_dir, "PROTOCOL_VERSION")

def _hash_fragment_set(rel_paths):
    """sha256 over sorted (rel_path \\0 sha256(bytes) \\0) — canonical across Python and Rust."""
    h = hashlib.sha256()
    for rel in sorted(rel_paths):
        full = os.path.join(claudemd_dir, rel)
        with open(full, "rb") as f:
            content = f.read()
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(hashlib.sha256(content).hexdigest().encode("ascii"))
        h.update(b"\0")
    return h.hexdigest()

def compute_protocol_core_hash():
    core = manifest.get("protocol_core", [])
    return _hash_fragment_set(core) if core else ""

def compute_role_fragments_hash(sections):
    return _hash_fragment_set(sections)

def read_protocol_version():
    if not os.path.exists(PROTOCOL_VERSION_PATH):
        return "0.0"
    with open(PROTOCOL_VERSION_PATH) as f:
        return f.read().strip()

def write_protocol_version(v):
    with open(PROTOCOL_VERSION_PATH, "w") as f:
        f.write(v + "\n")

def build_header_lines(version, now, protocol_version, core_hash, role_fragments_hash):
    return [
        f"<!-- GENERATED by claudemd-gen.sh | v{version} | {now} | DO NOT EDIT DIRECTLY -->",
        f"<!-- chorus-prompt: {protocol_version} -->",
        f"<!-- protocol-core: sha256={core_hash} -->",
        f"<!-- role-fragments: sha256={role_fragments_hash} -->",
    ]

# #2311: compute protocol contract state once per run (before any role assembly)
protocol_core_hash = compute_protocol_core_hash()
protocol_version = read_protocol_version()
_stored_core = load_checksums().get("_protocol_core_hash")
if (protocol_core_hash and _stored_core and _stored_core != protocol_core_hash
        and mode not in ("dry-run", "diff", "check", "validate")):
    _x, _y = protocol_version.split(".")
    protocol_version = f"{_x}.{int(_y) + 1}"
    write_protocol_version(protocol_version)

def substitute_vars(content, role_vars):
    """Replace {{VAR_NAME}} with role-specific values"""
    for key, value in role_vars.items():
        placeholder = "{{" + key + "}}"
        content = content.replace(placeholder, value)
    return content

def check_unresolved(content, role_name):
    """Warn about any remaining {{VAR}} placeholders"""
    import re
    remaining = re.findall(r'\{\{[A-Z_]+\}\}', content)
    if remaining:
        unique = set(remaining)
        print(f"  WARNING [{role_name}]: Unresolved variables: {', '.join(unique)}", file=sys.stderr)
    return remaining

# --- Auto-bump: if inputs changed since last generation, bump patch version ---
def auto_bump_version(manifest, manifest_path, roles, variables, claudemd_dir):
    """Check if inputs changed since last generation. If yes, bump patch version."""
    stored = load_checksums()
    if not stored:
        return manifest["version"]  # No baseline yet, don't bump on first run

    changed = False
    for role_name, role_config in roles.items():
        current_hash = compute_input_hash(role_name, role_config, variables, manifest_path, claudemd_dir)
        stored_hash = stored.get(role_name, {}).get("input_hash", "")
        if current_hash != stored_hash:
            changed = True
            break

    if not changed:
        return manifest["version"]

    # Bump version (monotonic integer)
    cur = manifest["version"]
    if "." in cur:
        # Legacy semver — extract patch number and convert to plain integer
        new_version = str(int(cur.split(".")[-1]) + 1)
    else:
        new_version = str(int(cur) + 1)

    # Update manifest in memory and on disk
    manifest["version"] = new_version

    # Add changelog entry
    changelog = manifest.get("changelog", [])
    changelog.insert(0, {
        "version": new_version,
        "card": "#auto",
        "date": datetime.now().strftime("%Y-%m-%d"),
        "summary": "Auto-bump: fragment or variable changes detected"
    })
    manifest["changelog"] = changelog

    # Write updated manifest
    with open(manifest_path, 'w') as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")

    return new_version

if mode == "generate":
    # Auto-bump disabled by default — bumping on any input hash change makes
    # manifest version a session counter, not a version. Per manifest's own
    # _versioning_rule, bumps are for protocol changes. Use CLAUDEMD_BUMP=1
    # (or mode == "bump") to bump explicitly when a real protocol change
    # ships. The regression lock test asserts this invariant.
    old_version = version
    if os.environ.get("CLAUDEMD_BUMP") == "1":
        version = auto_bump_version(manifest, manifest_path, manifest["roles"], manifest["variables"], claudemd_dir)
        if version != old_version:
            print(f"  Bump: v{old_version} → v{version}", file=sys.stderr)
    header_comment = f"<!-- GENERATED by claudemd-gen.sh | v{version} | {now} | DO NOT EDIT DIRECTLY -->"

    # #2150: fragment fitness linter runs on every bump.
    # R4 (asymmetric fragments) and R5 (dangling DEC citations) block generation.
    # Other findings (R6 line-count variance, etc.) report but do not block.
    # Override: CLAUDEMD_LINT_SOFT=1 reports without blocking (emergency escape hatch).
    linter = os.path.join(os.path.dirname(os.path.abspath(__file__)), "lint-fragments.sh")
    if os.path.exists(linter):
        try:
            lint_out = subprocess.run(
                [linter, "--fixture", claudemd_dir],
                capture_output=True, text=True, timeout=30
            )
            if lint_out.stdout.strip():
                print("  lint-fragments findings:", file=sys.stderr)
                for line in lint_out.stdout.strip().splitlines():
                    print(f"    {line}", file=sys.stderr)
                blocking = [l for l in lint_out.stdout.splitlines() if re.match(r'^R[45] \[error\]', l)]
                if blocking:
                    if os.environ.get("CLAUDEMD_LINT_SOFT") == "1":
                        print("  ============================================================", file=sys.stderr)
                        print(f"  !! LINT BYPASS: CLAUDEMD_LINT_SOFT=1 set — {len(blocking)} R4/R5 error(s) ignored", file=sys.stderr)
                        print("  ============================================================", file=sys.stderr)
                        spine = os.path.join(os.path.dirname(os.path.abspath(__file__)), "chorus-log")
                        if os.path.exists(spine):
                            subprocess.run([spine, "claudemd.lint.bypassed", "system",
                                            f"blocking_count={len(blocking)}", "override=CLAUDEMD_LINT_SOFT"],
                                           capture_output=True, timeout=5)
                    else:
                        print(f"  lint-fragments: {len(blocking)} blocking finding(s) (R4/R5 errors). Fix or set CLAUDEMD_LINT_SOFT=1 to override.", file=sys.stderr)
                        sys.exit(2)
        except (FileNotFoundError, subprocess.TimeoutExpired) as e:
            print(f"  lint-fragments: {e}", file=sys.stderr)

errors = 0
generated = {}

for role_name, role_config in roles.items():
    output_rel = role_config["output"]
    sections = role_config["sections"]
    role_vars = dict(variables[role_name])  # copy so global injection doesn't leak across roles
    # #2311: inject protocol version so fragments can reference {{CHORUS_PROMPT_VERSION}}.
    # Identical across roles by construction — drives the chat-prompt header so Jeff
    # sees the version in every response, not only buried in an HTML comment.
    role_vars["CHORUS_PROMPT_VERSION"] = protocol_version
    output_path = resolve_output_path(output_rel)

    if mode == "dry-run":
        print(f"\n{role_name}:")
        print(f"  Output: {output_path}")
        print(f"  Sections: {len(sections)}")
        for s in sections:
            frag_path = os.path.join(claudemd_dir, s)
            exists = "OK" if os.path.exists(frag_path) else "MISSING"
            print(f"    [{exists}] {s}")
        print(f"  Variables: {', '.join(role_vars.keys())}")
        continue

    # Assemble content
    role_frags_hash = compute_role_fragments_hash(sections)
    parts = build_header_lines(version, now, protocol_version, protocol_core_hash, role_frags_hash) + [""]
    for section_path in sections:
        fragment = read_fragment(section_path)
        fragment = substitute_vars(fragment, role_vars)
        # Strip trailing whitespace from fragment, add single blank line between sections
        parts.append(fragment.rstrip())
        parts.append("")

    content = "\n".join(parts).rstrip() + "\n"
    check_unresolved(content, role_name)
    generated[role_name] = (content, output_path)

if mode == "dry-run":
    print(f"\nManifest version: {version}")
    print(f"Total roles: {len(roles)}")
    sys.exit(0)

if mode == "diff":
    any_diff = False
    for role_name, (content, output_path) in generated.items():
        if not os.path.exists(output_path):
            print(f"\n{role_name}: {output_path} does not exist (would be created)")
            any_diff = True
            continue

        # Write generated to temp file, diff against current
        with tempfile.NamedTemporaryFile(mode='w', suffix='.md', delete=False) as tmp:
            tmp.write(content)
            tmp_path = tmp.name

        result = subprocess.run(
            ["diff", "-u", "--label", f"{role_name}/current", output_path,
             "--label", f"{role_name}/generated", tmp_path],
            capture_output=True, text=True
        )
        os.unlink(tmp_path)

        if result.returncode != 0:
            any_diff = True
            # Count changed lines
            added = sum(1 for l in result.stdout.splitlines() if l.startswith('+') and not l.startswith('+++'))
            removed = sum(1 for l in result.stdout.splitlines() if l.startswith('-') and not l.startswith('---'))
            print(f"\n{'='*60}")
            print(f"{role_name}: +{added} -{removed} lines changed")
            print(f"{'='*60}")
            print(result.stdout)
        else:
            print(f"{role_name}: no changes")

    if any_diff:
        print(f"\nDiffs found. Run 'claudemd-gen.sh' to apply.")
    else:
        print(f"\nAll files up to date.")
    sys.exit(0)

if mode == "check":
    stored = load_checksums()
    stale = []
    stale_details = {}

    if not stored:
        # No checksums file — fall back to content comparison
        for role_name, (content, output_path) in generated.items():
            if not os.path.exists(output_path):
                stale.append(role_name)
                stale_details[role_name] = ["output file missing"]
                continue
            with open(output_path) as f:
                current = f.read()
            current_body = "\n".join(current.splitlines()[1:]) if current.startswith("<!--") else current
            generated_body = "\n".join(content.splitlines()[1:])
            if current_body.strip() != generated_body.strip():
                stale.append(role_name)
                stale_details[role_name] = ["content mismatch (no checksums — run generate to baseline)"]
    else:
        # Fast path: compare input hashes
        for role_name in roles:
            role_config = roles[role_name]
            current_hash = compute_input_hash(role_name, role_config, variables, manifest_path, claudemd_dir)
            stored_hash = stored.get(role_name, {}).get("input_hash", "")

            if current_hash != stored_hash:
                stale.append(role_name)
                changed = find_changed_fragments(role_name, role_config, claudemd_dir, stored)
                # Also check if variables changed
                current_var_hash = hashlib.sha256(json.dumps(variables.get(role_name, {}), sort_keys=True).encode()).hexdigest()
                stored_var_hash = stored.get(role_name, {}).get("var_hash", "")
                if current_var_hash != stored_var_hash:
                    changed.append("(variables changed)")
                # Check if role config (sections list) changed
                current_config_hash = hashlib.sha256(json.dumps(role_config, sort_keys=True).encode()).hexdigest()
                stored_config_hash = stored.get(role_name, {}).get("config_hash", "")
                if current_config_hash != stored_config_hash:
                    changed.append("(sections list changed)")
                stale_details[role_name] = changed if changed else ["unknown change"]

    if stale:
        print(f"STALE: {', '.join(stale)} — run claudemd-gen.sh to regenerate")
        for role_name in stale:
            details = stale_details.get(role_name, [])
            if details:
                print(f"  {role_name}: {', '.join(details)}")
        sys.exit(1)
    else:
        # Success is silent (#623) — callers grep for STALE:, absence means clean
        sys.exit(0)

if mode == "compat":
    # Read version from each generated CLAUDE.md header and compare to manifest
    messages_dir = claudemd_dir  # #2150: paths are relative to claudemd_dir, not its parent
    mismatches = []
    for role_name, role_config in manifest["roles"].items():
        output_path = os.path.normpath(os.path.join(messages_dir, role_config["output"]))
        output_ver = read_output_version(output_path)
        status = "OK" if output_ver == version else "MISMATCH"
        display_ver = output_ver or "missing"
        print(f"  {role_name}: v{display_ver} {status}")
        if output_ver != version:
            mismatches.append(role_name)

    # Show changelog for context
    changelog = manifest.get("changelog", [])
    if changelog:
        print(f"\nChangelog (last 5):")
        for entry in changelog[:5]:
            print(f"  v{entry['version']} ({entry['date']}) {entry['card']}: {entry['summary']}")

    # Show archived versions
    archived = list_version_snapshots()
    if archived:
        print(f"\nArchived versions: {', '.join(archived)}")

    if mismatches:
        print(f"\nCOMPAT: {len(mismatches)} role(s) out of sync — {', '.join(mismatches)} not at v{version}")
        sys.exit(1)
    else:
        print(f"\nCOMPAT: all roles at v{version}")
        sys.exit(0)

if mode == "history":
    archived = list_version_snapshots()
    changelog = manifest.get("changelog", [])

    if not archived and not changelog:
        print("No version history available. Generate at least once to create a baseline.")
        sys.exit(0)

    # Show changelog
    if changelog:
        print(f"Changelog ({len(changelog)} entries):")
        for entry in changelog:
            print(f"  v{entry['version']} | {entry['date']} | {entry['card']} | {entry['summary']}")

    # If we have 2+ snapshots, diff the last two
    if len(archived) >= 2:
        v1, v2 = archived[-2], archived[-1]
        s1 = load_version_snapshot(v1)
        s2 = load_version_snapshot(v2)
        if s1 and s2:
            print(f"\nDiff: v{v1} → v{v2}")
            changes = diff_versions(s1, s2)
            if changes:
                for role, role_changes in changes.items():
                    print(f"  {role}:")
                    for c in role_changes:
                        print(f"    {c}")
            else:
                print("  (no fragment changes)")
    elif len(archived) == 1:
        v = archived[0]
        snap = load_version_snapshot(v)
        if snap:
            print(f"\nBaseline: v{v} ({snap.get('archived_at', 'unknown')})")
            for role, data in snap.get("roles", {}).items():
                print(f"  {role}: {len(data.get('fragments', {}))} fragments")
    else:
        print("\nNo version snapshots yet. Run generate to create baseline.")

    sys.exit(0)

if mode == "pipeline":
    # Full orchestration: validate → detect drift → generate → verify → report → attach to card
    pipeline_result = {
        "timestamp": now,
        "version": version,
        "card": card_ref or None,
        "steps": [],
        "status": "PASS",
    }

    def step(name, status, detail=""):
        pipeline_result["steps"].append({"name": name, "status": status, "detail": detail})
        icon = "PASS" if status == "pass" else "FAIL" if status == "fail" else "WARN"
        print(f"  [{icon}] {name}{': ' + detail if detail else ''}")
        if status == "fail":
            pipeline_result["status"] = "FAIL"

    print(f"Pipeline: v{version}{' (card ' + card_ref + ')' if card_ref else ''}")
    print(f"{'='*50}")

    # Step 1: Validate
    v_errors, v_warnings = validate_manifest(manifest, claudemd_dir)
    if v_errors:
        step("validate", "fail", f"{len(v_errors)} error(s)")
        for e in v_errors:
            print(f"         {e}")
        # Write result even on failure
        pipeline_result["generated"] = []
        result_path = f"/tmp/claudemd-pipeline-{now.replace(' ', '-').replace(':', '')}.json"
        with open(result_path, 'w') as f:
            json.dump(pipeline_result, f, indent=2)
        print(f"\nPipeline: FAIL (validation errors)")
        print(f"Result: {result_path}")
        sys.exit(1)
    else:
        step("validate", "pass", f"9/9 checks ({len(v_warnings)} warning(s))")

    # Step 2: Detect drift
    stored = load_checksums()
    stale_roles = []
    changed_fragments = {}
    if stored:
        for role_name in roles:
            role_config = roles[role_name]
            current_hash = compute_input_hash(role_name, role_config, variables, manifest_path, claudemd_dir)
            stored_hash = stored.get(role_name, {}).get("input_hash", "")
            if current_hash != stored_hash:
                stale_roles.append(role_name)
                changed = find_changed_fragments(role_name, role_config, claudemd_dir, stored)
                changed_fragments[role_name] = changed
    else:
        stale_roles = list(roles.keys())

    if stale_roles:
        all_changed = set()
        for frags in changed_fragments.values():
            all_changed.update(frags)
        step("drift", "warn", f"{len(stale_roles)} role(s) stale — {', '.join(sorted(all_changed)) if all_changed else 'no checksums baseline'}")
    else:
        step("drift", "pass", "all roles current")

    # Step 3: Generate
    gen_results = {}
    gen_errors = 0
    messages_dir = claudemd_dir  # #2150: paths are relative to claudemd_dir, not its parent
    for role_name, role_config in roles.items():
        output_rel = role_config["output"]
        sections = role_config["sections"]
        role_vars = variables[role_name]
        output_path = os.path.normpath(os.path.join(messages_dir, output_rel))

        role_frags_hash = compute_role_fragments_hash(sections)
        parts = build_header_lines(version, now, protocol_version, protocol_core_hash, role_frags_hash) + [""]
        for section_path in sections:
            fragment = read_fragment(section_path)
            fragment = substitute_vars(fragment, role_vars)
            parts.append(fragment.rstrip())
            parts.append("")
        content = "\n".join(parts).rstrip() + "\n"
        check_unresolved(content, role_name)

        output_dir = os.path.dirname(output_path)
        if not os.path.exists(output_dir):
            gen_errors += 1
            continue
        with open(output_path, 'w') as f:
            f.write(content)
        line_count = len(content.splitlines())
        gen_results[role_name] = {"path": output_path, "lines": line_count}

    if gen_errors:
        step("generate", "fail", f"{gen_errors} error(s)")
    else:
        summary = ", ".join(f"{r} {d['lines']}L" for r, d in gen_results.items())
        step("generate", "pass", summary)

    # Step 4: Save checksums + snapshot
    checksums = {}
    for role_name, role_config in manifest["roles"].items():
        checksums[role_name] = {
            "input_hash": compute_input_hash(role_name, role_config, variables, manifest_path, claudemd_dir),
            "var_hash": hashlib.sha256(json.dumps(variables.get(role_name, {}), sort_keys=True).encode()).hexdigest(),
            "config_hash": hashlib.sha256(json.dumps(role_config, sort_keys=True).encode()).hexdigest(),
            "fragments": compute_fragment_hashes(role_name, role_config, claudemd_dir),
            "generated_at": now,
            "version": version,
        }
    # #2311: persist protocol-core hash alongside per-role checksums
    if protocol_core_hash:
        checksums["_protocol_core_hash"] = protocol_core_hash
        checksums["_protocol_version"] = protocol_version
    save_checksums(checksums)
    save_version_snapshot(version, manifest["roles"], variables, claudemd_dir)
    step("checksums", "pass", f"saved + v{version} snapshot archived")

    # Step 5: Verify (re-check that output matches)
    verify_ok = True
    for role_name, role_config in manifest["roles"].items():
        output_path = os.path.normpath(os.path.join(messages_dir, role_config["output"]))
        output_ver = read_output_version(output_path)
        if output_ver != version:
            verify_ok = False
    if verify_ok:
        step("verify", "pass", f"all outputs at v{version}")
    else:
        step("verify", "fail", "version mismatch in output headers")

    # Build pipeline result
    pipeline_result["generated"] = gen_results
    pipeline_result["changed_fragments"] = {r: frags for r, frags in changed_fragments.items()}
    pipeline_result["stale_roles"] = stale_roles

    # Write result to temp file + persistent archive
    ts_slug = now.replace(' ', '-').replace(':', '')
    result_path = f"/tmp/claudemd-pipeline-{ts_slug}.json"
    with open(result_path, 'w') as f:
        json.dump(pipeline_result, f, indent=2)

    # Archive to persistent location
    archive_dir = os.path.join(claudemd_dir, "pipeline-runs")
    os.makedirs(archive_dir, exist_ok=True)
    archive_path = os.path.join(archive_dir, f"{ts_slug}.json")
    with open(archive_path, 'w') as f:
        json.dump(pipeline_result, f, indent=2)

    print(f"{'='*50}")
    print(f"Pipeline: {pipeline_result['status']} (v{version})")
    print(f"Result: {result_path}")

    # Step 6: Attach to card if specified
    if card_ref and pipeline_result["status"] != "FAIL":
        # Build a concise comment for the card
        comment_parts = [f"Pipeline v{version} | {now}"]
        for s in pipeline_result["steps"]:
            icon = "+" if s["status"] == "pass" else "!" if s["status"] == "warn" else "x"
            comment_parts.append(f"  [{icon}] {s['name']}: {s.get('detail', '')}")
        if changed_fragments:
            all_changed = set()
            for frags in changed_fragments.values():
                all_changed.update(frags)
            if all_changed:
                comment_parts.append(f"  Changed: {', '.join(sorted(all_changed))}")
        comment = "\n".join(comment_parts)

        # Post to board
        board_ts = os.path.join(os.path.dirname(claudemd_dir), "scripts", "cards")
        try:
            result = subprocess.run(
                [board_ts, "comment", card_ref.lstrip('#'), comment],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                print(f"Attached to card {card_ref}")
            else:
                print(f"Card comment failed: {result.stderr.strip()}", file=sys.stderr)
        except Exception as e:
            print(f"Card comment error: {e}", file=sys.stderr)

    sys.exit(0 if pipeline_result["status"] == "PASS" else 1)

# Default: generate
for role_name, (content, output_path) in generated.items():
    output_dir = os.path.dirname(output_path)
    if not os.path.exists(output_dir):
        print(f"  ERROR: Output directory does not exist: {output_dir}", file=sys.stderr)
        errors += 1
        continue

    with open(output_path, 'w') as f:
        f.write(content)
    line_count = len(content.splitlines())
    print(f"  {role_name}: {output_path} ({line_count} lines)", file=sys.stderr)

# Generate TEAM_PROTOCOL.md from reference sections
if reference and mode in ("generate", "pipeline"):
    ref_output = resolve_output_path(reference["output"])
    ref_parts = [f"<!-- GENERATED by claudemd-gen.sh | v{version} | {now} | DO NOT EDIT DIRECTLY -->", ""]
    ref_parts.append("# Team Protocol Reference")
    ref_parts.append("")
    ref_parts.append("Shared reference material for all roles. Loaded on demand, not memorized.")
    ref_parts.append("See each role's CLAUDE.md for behavioral rules and identity.")
    ref_parts.append("")
    for section_path in reference.get("sections", []):
        fragment = read_fragment(section_path)
        ref_parts.append(fragment.rstrip())
        ref_parts.append("")
    ref_content = "\n".join(ref_parts).rstrip() + "\n"
    ref_dir = os.path.dirname(ref_output)
    if os.path.exists(ref_dir):
        with open(ref_output, 'w') as f:
            f.write(ref_content)
        ref_lines = len(ref_content.splitlines())
        print(f"  reference: {ref_output} ({ref_lines} lines, {len(ref_content)} chars)", file=sys.stderr)
    else:
        print(f"  ERROR: Reference output directory does not exist: {ref_dir}", file=sys.stderr)
        errors += 1

# Size budget check
for role_name, (content, output_path) in generated.items():
    char_count = len(content)
    if char_count > size_budget:
        print(f"  BUDGET [{role_name}]: {char_count} chars > {size_budget} budget", file=sys.stderr)
    elif char_count > size_budget * 0.9:
        print(f"  BUDGET [{role_name}]: {char_count} chars — {size_budget - char_count} headroom (warn >90%)", file=sys.stderr)

if errors:
    print(f"\n{errors} error(s) during generation.", file=sys.stderr)
    sys.exit(1)
else:
    # Save checksums for drift detection
    checksums = {}
    for role_name, role_config in manifest["roles"].items():
        if role_filter and role_name != role_filter:
            continue
        checksums[role_name] = {
            "input_hash": compute_input_hash(role_name, role_config, variables, manifest_path, claudemd_dir),
            "var_hash": hashlib.sha256(json.dumps(variables.get(role_name, {}), sort_keys=True).encode()).hexdigest(),
            "config_hash": hashlib.sha256(json.dumps(role_config, sort_keys=True).encode()).hexdigest(),
            "fragments": compute_fragment_hashes(role_name, role_config, claudemd_dir),
            "generated_at": now,
            "version": version,
        }
    # Merge with existing checksums (for single-role regeneration)
    existing = load_checksums()
    existing.update(checksums)
    # #2311: persist protocol contract state so the next regen can detect core-hash
    # drift and auto-bump Y. Without this, _stored_core stays None and the
    # auto-bump at module-top short-circuits forever.
    if protocol_core_hash:
        existing["_protocol_core_hash"] = protocol_core_hash
        existing["_protocol_version"] = protocol_version
    save_checksums(existing)

    # Archive version snapshot (always — overwrites same version, captures new on bump)
    snap_path = save_version_snapshot(version, manifest["roles"], variables, claudemd_dir)
    # Success is silent (#623) — generation complete


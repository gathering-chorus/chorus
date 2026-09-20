"""Plan explicit activation of known Chorus services, without changing live state.

The installer builds native Node dependencies with one selected executable. A
service must use that same runtime and checkout when those builds are activated.
Unknown launch commands need operator review; never rewrite arbitrary shell code.
"""
from copy import deepcopy
from pathlib import Path


class ServicePlanError(ValueError):
    """A service cannot be safely mapped onto the selected deployment."""


NODE_SERVICES = {
    "com.chorus.api": ("platform/api", "server.js", "chorus-api-wrapper.sh"),
    "com.gathering.messaging": ("platform/pulse", "service.js", None),
    "com.chorus.mcp": ("platform/mcp-server", "main.js", "chorus-mcp-wrapper.sh"),
}
SHELLS = {"/bin/bash", "/bin/sh", "/bin/zsh"}


def _refuse(label, detail):
    raise ServicePlanError(f"{label}: {detail}; review this LaunchAgent before --restart-services")


def _arguments(label, value):
    args = value.get("ProgramArguments")
    if args is None and isinstance(value.get("Program"), str):
        args = [value["Program"]]
    if not isinstance(args, list) or not args or any(not isinstance(arg, str) or not arg for arg in args):
        _refuse(label, "missing or invalid ProgramArguments")
    # launchd executes Program when present, even if argv[0] says otherwise.
    if value.get("Program") is not None and value["Program"] != args[0]:
        _refuse(label, "Program overrides the declared command")
    return list(args)


def _wrapper_index(args, wrapper):
    if wrapper and Path(args[0]).name == wrapper:
        return 0
    if wrapper and args[0] in SHELLS and len(args) > 1 and Path(args[1]).name == wrapper:
        return 1
    return None


def _script_index(label, args, directory, entry, working_directory):
    if Path(args[0]).name not in {"node", "nodejs"}:
        _refuse(label, "unrecognized wrapper or runtime")
    expected_suffix = f"{directory}/dist/{entry}"
    matches = []
    for index, arg in enumerate(args[1:], 1):
        recognized = arg.endswith("/" + expected_suffix)
        recognized |= arg == f"dist/{entry}"
        recognized |= arg == entry and str(working_directory or "").endswith(f"{directory}/dist")
        if recognized:
            matches.append(index)
    if len(matches) != 1:
        _refuse(label, f"cannot identify the expected {expected_suffix} entry point")
    index = matches[0]
    # Support normal runtime flags, not shell/eval/preload commands or positional
    # expressions that could turn the expected entry point into a mere argument.
    forbidden = {"-e", "--eval", "-p", "--print", "-r", "--require", "--import", "--loader", "--experimental-loader"}
    if any(not arg.startswith("-") or arg.split("=", 1)[0] in forbidden for arg in args[1:index]):
        _refuse(label, "unsupported arguments before the Node entry point")
    return index


def prepare_services(services: dict, root: Path, node: str) -> dict:
    """Return activation copies of ``label: (plist_path, plist_dict)``.

    No commands, filesystem writes, or service activation occur here. Existing
    custom arguments, logs, scheduling, authentication settings and environment
    are retained. Call before building when explicit activation was requested.
    """
    root = Path(root)
    node_path = Path(node)
    if not root.is_absolute() or not node_path.is_absolute():
        raise ServicePlanError("Deployment checkout and selected Node executable must be absolute paths")
    planned = deepcopy(services)
    for label, (path, value) in planned.items():
        if not isinstance(value, dict) or value.get("Label") != label:
            _refuse(label, "plist Label does not match the selected service")
        args = _arguments(label, value)
        if label == "com.chorus.hooks":
            if Path(args[0]).name != "chorus-hooks":
                _refuse(label, "unrecognized hooks launcher")
            args[0] = str(Path.home() / ".chorus/bin/chorus-hooks")
            directory = ""
        elif label in NODE_SERVICES:
            directory, entry, wrapper = NODE_SERVICES[label]
            wrapper_index = _wrapper_index(args, wrapper)
            if wrapper_index is not None:
                args[wrapper_index] = str(root / "platform/scripts" / wrapper)
            else:
                index = _script_index(label, args, directory, entry, value.get("WorkingDirectory"))
                args[0] = str(node_path)
                args[index] = str(root / directory / "dist" / entry)
        else:
            _refuse(label, "service is outside the supported activation set")
        if "Program" in value:
            value["Program"] = args[0]
        value["ProgramArguments"] = args
        value["WorkingDirectory"] = str(root / directory)
        environment = value.setdefault("EnvironmentVariables", {})
        if not isinstance(environment, dict):
            _refuse(label, "invalid EnvironmentVariables")
        previous_path = environment.get("PATH", "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
        if not isinstance(previous_path, str):
            _refuse(label, "invalid PATH environment value")
        path_entries = [str(node_path.parent), str(Path.home() / ".chorus/bin"), str(root / "platform/scripts")]
        path_entries.extend(previous_path.split(":"))
        environment.update({
            "CHORUS_NODE_BIN": str(node_path),
            "CHORUS_ROOT": str(root),
            "CHORUS_HOME": str(root),
            "CHORUS_MCP_DIR": str(root / "platform/mcp-server"),
            "PATH": ":".join(dict.fromkeys(entry for entry in path_entries if entry)),
        })
    return planned

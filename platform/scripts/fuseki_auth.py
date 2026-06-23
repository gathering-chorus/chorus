"""fuseki_auth.py — #3566 LOCK: the python-side write door.

One place owns the credential logic for python Fuseki writers (the "one door"
principle, same as the bash fuseki-auth.sh helper and the TS client factories).

Usage:
    import sys, os
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from fuseki_auth import write_auth_headers
    headers = {"Content-Type": "application/x-www-form-urlencoded", **write_auth_headers()}

When FUSEKI_ADMIN_PASSWORD is unset, write_auth_headers() returns {} — the write
stays unauthenticated (current behavior), so this is safe to land before the shared
lock is flipped. Reads should NOT call this.
"""
import base64
import os


def write_auth_headers(env=None):
    """Return {'Authorization': 'Basic <b64>'} when a write credential is configured, else {}."""
    env = env if env is not None else os.environ
    password = env.get("FUSEKI_ADMIN_PASSWORD")
    if not password:
        return {}
    user = env.get("FUSEKI_ADMIN_USER", "admin")
    token = base64.b64encode(f"{user}:{password}".encode()).decode()
    return {"Authorization": f"Basic {token}"}

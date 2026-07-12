"""#3637 — photo writers carry Fuseki Basic auth (deploy-before-require gate for #3630).

Hermetic: no network, no live Fuseki, no live NiFi. The tests import the two
writer scripts from their repo paths and assert the auth-header behavior of the
one credential door (platform/scripts/fuseki_auth.py):
  - writes carry `Authorization: Basic ...` when FUSEKI_ADMIN_PASSWORD is set
  - writes stay anonymous when it is unset (safe pre-flip behavior)
  - reads NEVER carry the write credential
"""
import base64
import importlib.util
import os
import sys
import unittest
from unittest import mock

REPO = os.environ.get(
    "CHORUS_ROOT",
    os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")),
)


def _load(name, rel_path):
    spec = importlib.util.spec_from_file_location(name, os.path.join(REPO, rel_path))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _basic(user, password):
    return "Basic " + base64.b64encode(f"{user}:{password}".encode()).decode()


class PhotoPipelineAuthTest(unittest.TestCase):
    def setUp(self):
        self.pipeline = _load("photo_pipeline", "roles/kade/scripts/photo-pipeline.py")
        self.requests = []

        def fake_urlopen(req, timeout=None, **kwargs):
            self.requests.append(req)
            resp = mock.MagicMock()
            resp.status = 200
            resp.read.return_value = b'{"results":{"bindings":[]}}'
            return resp

        self.urlopen_patch = mock.patch.object(
            self.pipeline.urllib.request, "urlopen", side_effect=fake_urlopen
        )
        self.urlopen_patch.start()
        self.addCleanup(self.urlopen_patch.stop)

    def test_sparql_update_carries_basic_auth_when_cred_set(self):
        with mock.patch.dict(os.environ, {"FUSEKI_ADMIN_PASSWORD": "s3cret"}):
            self.pipeline.sparql_update("INSERT DATA {}")
        req = self.requests[-1]
        self.assertEqual(req.get_header("Authorization"), _basic("admin", "s3cret"))

    def test_load_graph_put_carries_basic_auth_when_cred_set(self):
        with mock.patch.dict(os.environ, {"FUSEKI_ADMIN_PASSWORD": "s3cret"}):
            with mock.patch("builtins.open", mock.mock_open(read_data=b"")):
                self.pipeline.load_graph("urn:jb:photos/test/", "/tmp/x.nt")
        req = self.requests[-1]
        self.assertEqual(req.get_method(), "PUT")
        self.assertEqual(req.get_header("Authorization"), _basic("admin", "s3cret"))

    def test_writes_stay_anonymous_when_cred_unset(self):
        env = {k: v for k, v in os.environ.items() if k != "FUSEKI_ADMIN_PASSWORD"}
        with mock.patch.dict(os.environ, env, clear=True):
            self.pipeline.sparql_update("INSERT DATA {}")
        self.assertIsNone(self.requests[-1].get_header("Authorization"))

    def test_read_path_never_carries_write_cred(self):
        with mock.patch.dict(os.environ, {"FUSEKI_ADMIN_PASSWORD": "s3cret"}):
            self.pipeline.sparql_query("SELECT * WHERE {} LIMIT 1")
        self.assertIsNone(self.requests[-1].get_header("Authorization"))


class NifiFlowAuthPropsTest(unittest.TestCase):
    def setUp(self):
        self.flow = _load("build_nifi_photos_flow", "roles/kade/scripts/build-nifi-photos-flow.py")

    def test_write_processor_props_include_basic_auth_when_cred_set(self):
        with mock.patch.dict(os.environ, {"FUSEKI_ADMIN_PASSWORD": "s3cret"}):
            props = self.flow.fuseki_basic_auth_props()
        self.assertEqual(props.get("Basic Authentication Username"), "admin")
        self.assertEqual(props.get("Basic Authentication Password"), "s3cret")

    def test_props_empty_when_cred_unset_safe_pre_flip(self):
        env = {k: v for k, v in os.environ.items() if k != "FUSEKI_ADMIN_PASSWORD"}
        with mock.patch.dict(os.environ, env, clear=True):
            self.assertEqual(self.flow.fuseki_basic_auth_props(), {})

    def test_user_overridable_via_env(self):
        with mock.patch.dict(
            os.environ,
            {"FUSEKI_ADMIN_PASSWORD": "s3cret", "FUSEKI_ADMIN_USER": "writer"},
        ):
            props = self.flow.fuseki_basic_auth_props()
        self.assertEqual(props.get("Basic Authentication Username"), "writer")


if __name__ == "__main__":
    unittest.main()

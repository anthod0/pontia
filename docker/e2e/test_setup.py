"""Run with: python3 -B -m unittest discover -s docker/e2e -p 'test_*.py'."""

import base64
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader("docker_e2e", str(ROOT / "scripts/docker-e2e"))
spec = importlib.util.spec_from_loader(loader.name, loader)
setup = importlib.util.module_from_spec(spec)
loader.exec_module(setup)


def openssl(*args):
    return subprocess.run(["openssl", *map(str, args)], check=True, capture_output=True).stdout


class ReferenceDataTests(unittest.TestCase):
    def test_seed_is_repeatable_preserves_words_and_creates_no_auth_resources(self):
        db = sqlite3.connect(":memory:")
        self.addCleanup(db.close)
        for migration in sorted((ROOT / "apps/cloud/migrations").glob("*/migration.sql")):
            db.executescript(migration.read_text())
        db.execute("INSERT INTO hero_name_modifiers (word) VALUES ('custom')")
        seed = (ROOT / "docker/e2e/cloud-seed.sql").read_text()
        db.executescript(seed)
        before = {table: db.execute(f"SELECT word FROM {table} ORDER BY word").fetchall()
                  for table in ("hero_name_modifiers", "hero_name_heroes")}
        self.assertGreater(len(before["hero_name_modifiers"]), 1)
        self.assertGreater(len(before["hero_name_heroes"]), 0)
        db.executescript(seed)
        for table, words in before.items():
            self.assertEqual(db.execute(f"SELECT word FROM {table} ORDER BY word").fetchall(), words)
        self.assertIn(("custom",), before["hero_name_modifiers"])
        for table in ("users", "edges", "edge_tickets", "devices"):
            self.assertEqual(db.execute(f"SELECT count(*) FROM {table}").fetchone()[0], 0)


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory(prefix="pontia-docker-e2e-test-")
        self.addCleanup(self.sandbox.cleanup)
        self.root = Path(self.sandbox.name)
        self.state = self.root / "state"
        setup.initialize(self.state)

    def oauth(self):
        path = self.state / "cloud.dev.vars"
        path.write_text(path.read_text().replace('GOOGLE_CLIENT_ID=""', 'GOOGLE_CLIENT_ID="test-id"')
                        .replace('GOOGLE_CLIENT_SECRET=""', 'GOOGLE_CLIENT_SECRET="test-secret"'))

    def certificates(self):
        ca = self.root / "ca.pem"
        ca_key = self.root / "ca.key"
        tls = self.state / "tls"
        openssl("req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256",
                "-nodes", "-keyout", ca_key, "-out", ca, "-days", "2", "-subj", "/CN=Test Root",
                "-addext", "basicConstraints=critical,CA:TRUE")
        csr = self.root / "server.csr"
        openssl("req", "-new", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256",
                "-nodes", "-keyout", tls / "privkey.pem", "-out", csr, "-subj", "/CN=pontia.dev")
        extensions = self.root / "extensions"
        extensions.write_text("subjectAltName=DNS:pontia.dev,DNS:app.pontia.dev,DNS:docker-e2e.edge.pontia.dev\n"
                              "extendedKeyUsage=serverAuth\n")
        openssl("x509", "-req", "-in", csr, "-CA", ca, "-CAkey", ca_key, "-CAcreateserial",
                "-out", tls / "fullchain.pem", "-days", "2", "-extfile", extensions)
        # Only OpenSSL in this test process trusts this temporary CA. HOME is untouched.
        return patch.dict(os.environ, {"SSL_CERT_FILE": str(ca)})

    def test_initialization_preserves_existing_keys_and_oauth_configuration(self):
        self.oauth()
        original = (self.state / "cloud.dev.vars").read_bytes()
        setup.initialize(self.state)
        self.assertEqual((self.state / "cloud.dev.vars").read_bytes(), original)
        self.assertEqual((self.state / "cloud.dev.vars").stat().st_mode & 0o777, 0o600)

    def test_openrouter_key_refreshes_native_configuration(self):
        path = self.state / "auth/device/claude-settings.json"
        setup.write_private(path, json.dumps({
            "model": "test-model", "env": {"ANTHROPIC_API_KEY": "old-key"},
        }))
        original = path.read_bytes()
        setup.prepare_claude(self.state)
        self.assertEqual(path.read_bytes(), original)
        config = self.state / "cloud.dev.vars"
        base = config.read_text()
        for key in ("test-openrouter-first", "test-openrouter-rotated"):
            config.write_text(base + f'OPENROUTER_API_KEY="{key}"\n')
            setup.prepare_claude(self.state)
            self.assertEqual(json.loads(path.read_text()), {
                "model": "test-model",
                "env": {
                    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                    "ANTHROPIC_AUTH_TOKEN": key,
                    "ANTHROPIC_API_KEY": "",
                },
            })
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_openrouter_placeholder_stops_startup_before_containers_are_created(self):
        config = self.state / "cloud.dev.vars"
        base = config.read_text()
        for value in ('xxx', '""', '"  "'):
            config.write_text(base + f"OPENROUTER_API_KEY={value}\n")
            with patch.object(setup, "STATE", self.state), \
                    patch.object(setup, "run", return_value=subprocess.CompletedProcess([], 0, b"")), \
                    patch.object(setup, "compose") as compose:
                with self.assertRaises(ValueError):
                    setup.up()
                compose.assert_not_called()
            self.assertFalse((self.state / "auth/device/claude-settings.json").exists())

    def test_saved_browser_credentials_exclude_local_auth_and_browsing_state(self):
        cookie_path = self.root / "Cookies"
        with sqlite3.connect(cookie_path) as db:
            self.addCleanup(db.close)
            db.execute("PRAGMA journal_mode=WAL")
            db.execute("CREATE TABLE cookies (host_key TEXT, value TEXT)")
            db.executemany("INSERT INTO cookies VALUES (?, ?)", [
                (".github.com", "provider-login"), ("accounts.google.com", "google-login"),
                ("pontia.dev", "local-login"), ("app.pontia.dev", "local-dashboard"),
                ("example.com", "unrelated"), ("notgithub.com", "unrelated"),
            ])
        files = {
            setup.AUTH_FILES["browser"]["Cookies"]: cookie_path.read_bytes(),
            setup.AUTH_FILES["browser"]["Cookies"] + "-wal": Path(str(cookie_path) + "-wal").read_bytes(),
            setup.AUTH_FILES["browser"]["Local State"]: json.dumps({
                "os_crypt": {"encrypted_key": "test-key"},
                "profile": {"last_used": "Default"}, "unrelated": "discard",
            }).encode(),
        }
        with patch.object(setup, "container_file", side_effect=lambda container, path: files.get(path)):
            setup.save_browser_credentials("test-browser", self.state)
        saved = self.state / "auth/browser/Cookies"
        with sqlite3.connect(saved) as db:
            self.assertEqual(db.execute("SELECT host_key, value FROM cookies ORDER BY host_key").fetchall(),
                             [(".github.com", "provider-login"), ("accounts.google.com", "google-login")])
        self.assertEqual(json.loads((self.state / "auth/browser/Local State").read_text()),
                         {"os_crypt": {"encrypted_key": "test-key"}})
        self.assertEqual(saved.stat().st_mode & 0o777, 0o600)

    def test_task_scope_cleans_up_after_success_failure_and_termination(self):
        active = self.root / "active-run"
        cases = [
            ("pass", None),
            ("raise SystemExit(7)", subprocess.CalledProcessError),
            ("import os,signal,time; os.kill(os.getppid(),signal.SIGTERM); time.sleep(30)", KeyboardInterrupt),
        ]
        for code, error in cases:
            with self.subTest(code=code), patch.object(setup, "ROOT", self.root), \
                    patch.object(setup, "up", side_effect=lambda: active.touch()), \
                    patch.object(setup, "down", side_effect=lambda: active.unlink()):
                if error:
                    with self.assertRaises(error):
                        setup.run_task([sys.executable, "-c", code])
                else:
                    setup.run_task([sys.executable, "-c", code])
                self.assertFalse(active.exists())

    def test_new_environments_have_independent_valid_signing_keys(self):
        other = self.root / "other"
        setup.initialize(other)
        first = setup.variables(self.state / "cloud.dev.vars")
        second = setup.variables(other / "cloud.dev.vars")
        self.assertNotEqual(first["JWT_SECRET"], second["JWT_SECRET"])
        for private_key, public_key in (
            ("E2E_CAPABILITY_SIGNING_KEY", "E2E_CAPABILITY_VERIFICATION_KEY"),
            ("E2E_REGISTRATION_PROOF_PRIVATE_KEY", "E2E_REGISTRATION_PROOF_PUBLIC_KEY"),
        ):
            self.assertNotEqual(first[private_key], second[private_key])
            private = base64.urlsafe_b64decode(first[private_key] + "==")
            public = subprocess.run(
                ["openssl", "pkey", "-inform", "DER", "-pubout", "-outform", "DER"],
                input=private, check=True, capture_output=True,
            ).stdout
            self.assertEqual(public[-32:], base64.urlsafe_b64decode(first[public_key] + "=="))

    def test_preflight_requires_oauth_and_forbids_public_dns_credentials(self):
        with self.assertRaises(ValueError):
            setup.check(self.state)
        self.oauth()
        path = self.state / "cloud.dev.vars"
        path.write_text(path.read_text().replace('CLOUDFLARE_DNS_TOKEN="disabled-in-local-e2e"',
                                                'CLOUDFLARE_DNS_TOKEN="do-not-use"'))
        with self.assertRaises(ValueError):
            setup.check(self.state)

    def test_preflight_accepts_matching_trusted_certificate(self):
        self.oauth()
        with self.certificates():
            setup.check(self.state)

    def test_certificate_validation_rejects_wrong_hostname_and_private_key(self):
        with self.certificates():
            with self.assertRaises(subprocess.CalledProcessError):
                setup.check_certificates(self.state, ["other.edge.pontia.dev"])
            openssl("genpkey", "-algorithm", "ED25519", "-out", self.state / "tls/privkey.pem")
            with self.assertRaises(ValueError):
                setup.check_certificates(self.state, ["pontia.dev"])

    def test_untrusted_certificate_is_not_accepted(self):
        self.certificates()  # Do not install the test CA into the process environment.
        with self.assertRaises(subprocess.CalledProcessError):
            setup.check_certificates(self.state, ["pontia.dev"])

    def test_public_edge_bootstrap_is_repeatable_and_new_runs_rotate_credentials(self):
        self.oauth()
        with self.certificates():
            setup.prepare_edge(self.state)
            setup.check(self.state)
        db = sqlite3.connect(":memory:")
        self.addCleanup(db.close)
        db.execute("PRAGMA foreign_keys=ON")
        for migration in sorted((ROOT / "apps/cloud/migrations").glob("*/migration.sql")):
            db.executescript(migration.read_text())
        seed = (self.state / "cloud-bootstrap.sql").read_text()
        db.executescript(seed)
        db.executescript(seed)  # Restarting Cloud must preserve the current registration.
        row = db.execute("SELECT id, access_scope, tunnel_url, service_credential_hash FROM edges").fetchone()
        self.assertEqual(db.execute("SELECT count(*) FROM edges").fetchone()[0], 1)
        self.assertEqual(row[:3], ("01900000-0000-7000-8000-000000000001", "public",
                                  "wss://docker-e2e.edge.pontia.dev/tunnel"))
        credential = (self.state / "edge/credential").read_text().strip()
        secret = credential.removeprefix(f"pec_v1_{row[0]}_")
        self.assertEqual(len(base64.urlsafe_b64decode(secret + "=")), 32)
        self.assertEqual(row[3], base64.urlsafe_b64encode(hashlib.sha256(secret.encode()).digest()).decode().rstrip("="))
        for table in ("accounts", "auth_sessions", "devices", "edge_tickets"):
            self.assertEqual(db.execute(f"SELECT count(*) FROM {table}").fetchone()[0], 0)
        setup.prepare_edge(self.state)
        self.assertNotEqual((self.state / "edge/credential").read_text().strip(), credential)
        self.assertNotEqual((self.state / "cloud-bootstrap.sql").read_text(), seed)

    @unittest.skipUnless(shutil.which("docker"), "Docker CLI required for Compose validation")
    def test_compose_pins_all_application_domains_and_only_publishes_loopback_browser(self):
        env = {key: value for key, value in os.environ.items()
               if not key.startswith("COMPOSE_")}
        result = subprocess.run([
            "docker", "compose",
            "-f", str(ROOT / "docker/e2e/compose.yaml"), "config", "--format", "json",
        ], env=env, check=True, capture_output=True)
        config = json.loads(result.stdout)
        for service in config["services"].values():
            hosts = service["extra_hosts"]
            if isinstance(hosts, list):
                hosts = dict(entry.split("=", 1) for entry in hosts)
            self.assertEqual(hosts, {
                "pontia.dev": "172.30.83.2", "app.pontia.dev": "172.30.83.3",
                "docker-e2e.edge.pontia.dev": "172.30.83.4",
            })
            for port in service.get("ports", []):
                self.assertEqual(port["host_ip"], "127.0.0.1")
                self.assertEqual(port["target"], 6080)
            for mount in service.get("volumes", []):
                if mount["type"] == "bind":
                    self.assertTrue(mount["read_only"])
                    self.assertFalse(mount["bind"]["create_host_path"])
        self.assertTrue(config["networks"]["lab"]["internal"])
        self.assertEqual(config["networks"]["lab"]["ipam"]["config"][0]["ip_range"], "172.30.83.128/25")
        self.assertEqual(set(config["services"]["edge"]["networks"]), {"lab"})


if __name__ == "__main__":
    unittest.main()

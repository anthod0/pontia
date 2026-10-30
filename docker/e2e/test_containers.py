"""Opt-in image smoke tests; no public certificates or external credentials used."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import unittest
import uuid

import test_setup


@unittest.skipUnless(os.environ.get("PONTIA_DOCKER_SMOKE") == "1", "Set PONTIA_DOCKER_SMOKE=1 after building images")
class ContainerSmokeTests(unittest.TestCase):
    def test_local_stack_bootstraps_public_edge_and_discards_run_data(self):
        existing = subprocess.run([
            "docker", "network", "ls", "--filter", "label=com.docker.compose.project=pontia-e2e-docker",
            "--format", "{{.Name}}",
        ], check=True, capture_output=True)
        self.assertFalse(existing.stdout.strip(),
                         "Run scripts/docker-e2e down first: the normal lab and smoke test share a fixed subnet")
        fixture = test_setup.SetupTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        fixture.oauth()
        fixture.certificates()
        state = fixture.state
        test_setup.setup.write_private(state / "auth/device/codex.json", '{"test_credential":"retained"}')
        tls = state / "tls"
        shutil.copyfile(fixture.root / "ca.pem", tls / "test-ca.pem")
        edge = state / "edge"
        test_setup.setup.prepare_edge(state)
        with test_setup.patch.dict(os.environ, {"SSL_CERT_FILE": str(fixture.root / "ca.pem")}):
            test_setup.setup.check(state)
        shutil.copyfile(fixture.root / "ca.pem", edge / "test-ca.pem")
        env = {key: value for key, value in os.environ.items() if not key.startswith("COMPOSE_")}
        rendered = subprocess.run([
            "docker", "compose", "-f", str(test_setup.ROOT / "docker/e2e/compose.yaml"),
            "config", "--format", "json",
        ], env=env, check=True, capture_output=True)
        config = json.loads(rendered.stdout)
        config.pop("name", None)
        # Each run owns a unique project, networks and volumes. Disable ALL egress.
        for volume in config["volumes"].values():
            volume.pop("name", None)
        for network in config["networks"].values():
            network.pop("name", None)
            network["internal"] = True
        for name, service in config["services"].items():
            service.pop("build")
            service["image"] = f"pontia-e2e-docker-{name}:latest"
            service.pop("ports", None)
            if name == "device":
                service["security_opt"] = [f"seccomp={test_setup.ROOT / 'docker/e2e/device-seccomp.json'}"]
            for mount in service.get("volumes", []):
                if mount["type"] == "bind":
                    mount["source"] = str(state / Path(mount["source"]).relative_to(test_setup.ROOT / "docker/e2e/.local"))
        # Trust the ephemeral CA ONLY for smoke-test HTTP probes. The browser and
        # Rust clients are unchanged; this does not establish remote-access E2E.
        for name in ("cloud", "dashboard"):
            probe = config["services"][name]["healthcheck"]["test"]
            probe[2:2] = ["--cacert", "/tls/test-ca.pem"]
        config["services"]["edge"]["healthcheck"]["test"][1] = (
            "curl --fail --silent --cacert /etc/pontia/edge/test-ca.pem "
            "--resolve docker-e2e.edge.pontia.dev:443:127.0.0.1 https://docker-e2e.edge.pontia.dev/healthz"
        )
        compose_file = fixture.root / "compose.json"
        compose_file.write_text(json.dumps(config))
        project = "pontia-smoke-" + uuid.uuid4().hex[:12]
        prefix = ["docker", "compose", "--project-name", project, "-f", str(compose_file)]

        def compose(*args):
            return subprocess.run([*prefix, *args], env=env, check=True, capture_output=True)

        try:
            compose("up", "-d", "--wait", "--wait-timeout", "180")
            public_edge = compose("exec", "-T", "cloud", "./node_modules/.bin/wrangler", "d1", "execute", "DB",
                                  "--local", "--persist-to", "/state", "--json", "--command",
                                  "SELECT access_scope, tunnel_url FROM edges;")
            self.assertEqual(json.loads(public_edge.stdout)[0]["results"], [{
                "access_scope": "public", "tunnel_url": "wss://docker-e2e.edge.pontia.dev/tunnel",
            }])
            local = compose("exec", "-T", "browser", "curl", "--fail", "--silent", "http://localhost:8080/dashboard/")
            self.assertIn(b"<html", local.stdout)
            vnc = compose("exec", "-T", "browser", "curl", "--fail", "--silent", "http://127.0.0.1:6080/vnc.html")
            self.assertIn(b"<html", vnc.stdout)
            compose("exec", "-T", "device", "pi", "--version")
            compose("exec", "-T", "device", "codex", "--version")
            sandbox = compose("exec", "-T", "device", "codex", "sandbox", "-c", 'sandbox_mode="workspace-write"',
                              "--", "sh", "-c", "printf sandbox-ok > /home/e2e/workspace/sandbox.txt && cat /home/e2e/workspace/sandbox.txt")
            self.assertEqual(sandbox.stdout, b"sandbox-ok")
            denied = subprocess.run([*prefix, "exec", "-T", "device", "codex", "sandbox", "-c", 'sandbox_mode="workspace-write"',
                                     "--", "sh", "-c", "printf forbidden > /home/e2e/outside-workspace.txt"],
                                    env=env, capture_output=True)
            self.assertNotEqual(denied.returncode, 0)
            oauth = compose(
                "exec", "-T", "cloud", "curl", "--silent", "--show-error", "--cacert", "/tls/test-ca.pem",
                "--dump-header", "-", "--output", "/dev/null", "--request", "POST",
                "--header", "Origin: https://pontia.dev", "https://pontia.dev/api/auth/google/login",
            ).stdout
            self.assertIn(b"303", oauth)
            self.assertIn(b"accounts.google.com", oauth)
            self.assertIn(b"https%3A%2F%2Fpontia.dev%2Fapi%2Fauth%2Fgoogle%2Fcallback", oauth)
            vocabulary = compose(
                "exec", "-T", "cloud", "./node_modules/.bin/wrangler", "d1", "execute", "DB",
                "--local", "--persist-to", "/state", "--json", "--command",
                "SELECT (SELECT count(*) FROM hero_name_modifiers) AS modifiers, "
                "(SELECT count(*) FROM hero_name_heroes) AS heroes;",
            )
            counts = json.loads(vocabulary.stdout)[0]["results"][0]
            self.assertGreater(counts["modifiers"], 0)
            self.assertGreater(counts["heroes"], 0)
            compose("exec", "-T", "device", "sh", "-c",
                    "test -f /home/e2e/.codex/auth.json && touch /home/e2e/workspace/discard-me")
            compose("exec", "-T", "cloud", "./node_modules/.bin/wrangler", "d1", "execute", "DB",
                    "--local", "--persist-to", "/state", "--command",
                    "INSERT INTO hero_name_modifiers (word) VALUES ('discard-me');")
            # Stopping Cloud must leave its domain pinned, not resolve production.
            compose("stop", "cloud")
            hosts = compose("exec", "-T", "device", "getent", "hosts", "pontia.dev").stdout
            self.assertEqual(hosts.split()[0], b"172.30.83.2")
            unavailable = subprocess.run([
                *prefix, "exec", "-T", "device", "curl", "--noproxy", "*", "--max-time", "3",
                "--fail", "https://pontia.dev/",
            ], env=env, capture_output=True)
            self.assertNotEqual(unavailable.returncode, 0)
            compose("down", "--volumes")
            compose("up", "-d", "--wait", "--wait-timeout", "180")
            credentials = compose("exec", "-T", "device", "sh", "-c",
                                  "test ! -e /home/e2e/workspace/discard-me && cat /home/e2e/.codex/auth.json")
            self.assertEqual(json.loads(credentials.stdout), {"test_credential": "retained"})
            fresh = compose("exec", "-T", "cloud", "./node_modules/.bin/wrangler", "d1", "execute", "DB",
                            "--local", "--persist-to", "/state", "--json", "--command",
                            "SELECT count(*) AS count FROM hero_name_modifiers WHERE word='discard-me';")
            self.assertEqual(json.loads(fresh.stdout)[0]["results"][0]["count"], 0)
        except Exception:
            logs = subprocess.run([*prefix, "logs", "--tail=50"], env=env, capture_output=True)
            print(logs.stdout.decode(errors="replace"))
            raise
        finally:
            compose("down", "--volumes")

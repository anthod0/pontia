"""Opt-in image smoke tests; no public certificates or external credentials used."""

from concurrent.futures import ThreadPoolExecutor
import ipaddress
import json
import os
from pathlib import Path
import shutil
import subprocess
import textwrap
import unittest
import uuid

import test_setup


BROWSER_COOKIES = """
const {webSocketDebuggerUrl} = await (await fetch("http://127.0.0.1:9222/json/version")).json();
const socket = new WebSocket(webSocketDebuggerUrl);
socket.onopen = () => socket.send(JSON.stringify({id: 1, method: "Storage.getCookies"}));
socket.onmessage = ({data}) => {
  const cookies = JSON.parse(data).result.cookies;
  console.log(JSON.stringify(cookies.map(({name, value, domain}) => ({name, value, domain}))));
  socket.close();
};
"""


class SmokeLab(test_setup.setup.Lab):
    def compose(self, args, **kwargs):
        rendered = super().compose(["config", "--format", "json"], capture_output=True)
        config = json.loads(rendered.stdout)
        config["networks"]["outbound"]["internal"] = True
        for name, service in config["services"].items():
            service.pop("build")
            if name == "device":
                service["security_opt"] = [f"seccomp={self.root / 'docker/e2e/device-seccomp.json'}"]
        # Only health probes trust the temporary CA; application TLS is unchanged.
        for name in ("cloud", "dashboard", "edge"):
            config["services"][name]["volumes"].append({
                "type": "bind", "source": str(self.config / "tls/test-ca.pem"),
                "target": "/e2e-test-ca.pem", "read_only": True,
            })
        for name in ("cloud", "dashboard"):
            config["services"][name]["healthcheck"]["test"][2:2] = ["--cacert", "/e2e-test-ca.pem"]
        config["services"]["edge"]["healthcheck"]["test"] = ["CMD-SHELL",
            "curl --fail --silent --cacert /e2e-test-ca.pem "
            "--resolve docker-e2e.edge.pontia.dev:443:127.0.0.1 https://docker-e2e.edge.pontia.dev/healthz"]
        self.state.mkdir(parents=True, exist_ok=True)
        compose_file = self.state / "smoke-compose.json"
        compose_file.write_text(json.dumps(config))
        env = {key: value for key, value in os.environ.items() if not key.startswith("COMPOSE_")}
        return subprocess.run(["docker", "compose", "--project-name", self.project,
                               "-f", str(compose_file), *args], env=env, check=True, **kwargs)


@unittest.skipUnless(os.environ.get("PONTIA_DOCKER_SMOKE") == "1", "Set PONTIA_DOCKER_SMOKE=1 after building images")
class ContainerSmokeTests(unittest.TestCase):
    def test_claude_gateway_converts_requests_without_reasoning_or_database(self):
        probe = textwrap.dedent('''
            import http.server
            import json
            from pathlib import Path
            import subprocess
            import tempfile
            import threading
            import time
            import urllib.request

            requests = []
            class OpenAI(http.server.BaseHTTPRequestHandler):
                def log_message(self, *args):
                    pass

                def do_POST(self):
                    body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                    requests.append((self.path, body))
                    response = {'id': 'chatcmpl-test', 'object': 'chat.completion',
                                'model': 'gpt-4o-mini', 'created': 1,
                                'choices': [{'index': 0, 'message': {'role': 'assistant',
                                            'content': 'gateway-ok'}, 'finish_reason': 'stop'}],
                                'usage': {'prompt_tokens': 5, 'completion_tokens': 3, 'total_tokens': 8}}
                    data = json.dumps(response).encode()
                    self.send_response(200)
                    self.send_header('Content-Type', 'application/json')
                    self.send_header('Content-Length', str(len(data)))
                    self.end_headers()
                    self.wfile.write(data)

            with tempfile.TemporaryDirectory(prefix='claude-gateway-test-') as directory, \
                    http.server.ThreadingHTTPServer(('127.0.0.1', 0), OpenAI) as upstream:
                root = Path(directory)
                key = root / 'key'
                key.write_text('upstream-fixture')
                settings = root / 'settings.json'
                runtime = root / 'runtime'
                subprocess.run(['python3', '/usr/local/lib/configure-claude-proxy.py',
                                str(settings), str(key), str(runtime)], check=True)
                config_path = runtime / 'config.yaml'
                config = json.loads(config_path.read_text())
                config['api-keys']['openai-compatibility'][0]['base-url'] = (
                    f'http://127.0.0.1:{upstream.server_port}/v1')
                config_path.write_text(json.dumps(config))
                threading.Thread(target=upstream.serve_forever, daemon=True).start()
                process = subprocess.Popen(['CLIProxyAPI', '-config', str(config_path), '-local-model'],
                                           cwd=runtime, stdout=subprocess.DEVNULL)
                try:
                    for _ in range(100):
                        assert process.poll() is None
                        try:
                            with urllib.request.urlopen('http://127.0.0.1:8317/', timeout=1):
                                break
                        except OSError:
                            time.sleep(.1)
                    body = {'model': 'gpt-4o-mini', 'max_tokens': 32,
                            'thinking': {'type': 'disabled'}, 'output_config': {'effort': 'medium'},
                            'messages': [{'role': 'user', 'content': 'hello'}]}
                    request = urllib.request.Request('http://127.0.0.1:8317/v1/messages',
                        data=json.dumps(body).encode(), headers={'Content-Type': 'application/json',
                        'x-api-key': config['access']['api-keys'][0]})
                    with urllib.request.urlopen(request, timeout=10) as response:
                        assert json.load(response)['content'][0]['text'] == 'gateway-ok'
                    assert requests[0][0] == '/v1/chat/completions'
                    assert requests[0][1]['model'] == 'gpt-4o-mini'
                    assert 'reasoning_effort' not in requests[0][1]
                finally:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                    upstream.shutdown()
                assert not list(runtime.rglob('*.db'))
        ''')
        result = subprocess.run([
            "docker", "run", "--rm", "--init", "--network", "none",
            "--entrypoint", "python3", f"{test_setup.setup.Lab(test_setup.ROOT, test_setup.ROOT).images}-device:latest", "-c", probe,
        ], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_local_d1_commands_exit_when_registry_does_not_respond(self):
        probe = textwrap.dedent('''
            import json
            import os
            from pathlib import Path
            import socketserver
            import subprocess
            import tempfile
            import threading

            release = threading.Event()

            class Registry(socketserver.BaseRequestHandler):
                def handle(self):
                    release.wait()

            class Server(socketserver.ThreadingTCPServer):
                daemon_threads = True

            with tempfile.TemporaryDirectory(prefix="d1-registry-test-") as root, \
                    Server(("127.0.0.1", 0), Registry) as registry:
                threading.Thread(target=registry.serve_forever, daemon=True).start()
                env = dict(os.environ, TMPDIR=root,
                           npm_registry=f"http://127.0.0.1:{registry.server_address[1]}")
                prefix = ["bun", "x", "--no-install", "wrangler", "d1"]
                local = ["DB", "--local", "--persist-to", str(Path(root) / "state")]

                def run(*args):
                    return subprocess.run([*prefix, *args], env=env, capture_output=True,
                                          text=True, timeout=30)

                try:
                    for _ in range(2):
                        migrated = run("migrations", "apply", *local)
                        assert migrated.returncode == 0, migrated.stdout + migrated.stderr
                    queried = run("execute", *local, "--json", "--command",
                                  "SELECT count(*) AS count FROM edges;")
                    assert queried.returncode == 0, queried.stdout + queried.stderr
                    assert json.loads(queried.stdout)[0]["results"][0]["count"] == 0
                    invalid = run("execute", *local, "--command", "SELECT * FROM missing_table;")
                    assert invalid.returncode != 0
                finally:
                    release.set()
                    registry.shutdown()
        ''')
        result = subprocess.run([
            "docker", "run", "--rm", "--init", "--network", "none",
            "--entrypoint", "python3", f"{test_setup.setup.Lab(test_setup.ROOT, test_setup.ROOT).images}-cloud:latest", "-c", probe,
        ], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_local_stack_bootstraps_public_edge_and_discards_run_data(self):
        fixture = test_setup.SetupTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        fixture.certificates()
        state = fixture.state
        pi_auth = {"openai": {"type": "oauth", "refresh": "expired-fixture"},
                   "openrouter": {"type": "api_key", "key": "router-fixture"}}
        test_setup.setup.write_private(state / "auth/device/pi.json", json.dumps(pi_auth))
        test_setup.setup.write_private(state / "auth/device/claude.json", '{"test_credential":"original"}')
        claude_settings = {"env": {
            "ANTHROPIC_BASE_URL": "http://127.0.0.1:1",
            "ANTHROPIC_AUTH_TOKEN": "fixture-not-a-secret",
        }}
        test_setup.setup.write_private(state / "auth/device/claude-settings.json", json.dumps(claude_settings))
        tls = state / "tls"
        shutil.copyfile(fixture.root / "ca.pem", tls / "test-ca.pem")
        first = SmokeLab(test_setup.ROOT, state, "smoke-" + uuid.uuid4().hex[:12])
        second = SmokeLab(test_setup.ROOT, state, "smoke-" + uuid.uuid4().hex[:12])
        trust = test_setup.patch.dict(os.environ, {"SSL_CERT_FILE": str(fixture.root / "ca.pem")})
        trust.start()
        self.addCleanup(trust.stop)

        def compose(*args):
            return first.compose(list(args), capture_output=True)

        try:
            with ThreadPoolExecutor(max_workers=2) as executor:
                list(executor.map(lambda lab: lab.up(), (first, second)))
            self.assertNotEqual(json.loads((first.state / "network.json").read_text()),
                                json.loads((second.state / "network.json").read_text()))
            second.compose(["exec", "-T", "device", "touch", "/home/e2e/workspace/keep-me"])
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
            imported_pi = compose("exec", "-T", "device", "cat", "/home/e2e/.pi/agent/auth.json")
            self.assertEqual(json.loads(imported_pi.stdout), pi_auth)
            compose("exec", "-T", "device", "codex", "--version")
            compose("exec", "-T", "device", "claude", "--version")
            auth = compose("exec", "-T", "device", "claude", "auth", "status")
            self.assertTrue(json.loads(auth.stdout)["loggedIn"])
            self.assertEqual(json.loads(auth.stdout)["configDirectory"], "/home/e2e/.claude")
            compose("exec", "-T", "device", "pontia", "client", "enable", "claude-agent")
            # Exercise the installed SDK, tools, approvals and native history with
            # a loopback model substitute. This project has no external network.
            compose("exec", "-T", "-e", "PONTIA_CLAUDE_SDK_TEST=1", "device", "node", "--test",
                    "/opt/pontia/claude-validation/dist/test/sdk.test.js")
            imported = compose("exec", "-T", "device", "cat", "/home/e2e/.claude/.credentials.json")
            self.assertEqual(json.loads(imported.stdout), {"test_credential": "original"})
            sandbox = compose("exec", "-T", "device", "codex", "sandbox", "-c", 'sandbox_mode="workspace-write"',
                              "--", "sh", "-c", "printf sandbox-ok > /home/e2e/workspace/sandbox.txt && cat /home/e2e/workspace/sandbox.txt")
            self.assertEqual(sandbox.stdout, b"sandbox-ok")
            with self.assertRaises(subprocess.CalledProcessError):
                compose("exec", "-T", "device", "codex", "sandbox", "-c", 'sandbox_mode="workspace-write"',
                        "--", "sh", "-c", "printf forbidden > /home/e2e/outside-workspace.txt")
            login = json.loads((first.state / "browser-login.json").read_text())
            cookies = compose("exec", "-T", "browser", "node", "--input-type=module", "-e", BROWSER_COOKIES).stdout
            self.assertIn({"name": "__Host-_rt", "value": login["value"], "domain": "pontia.dev"},
                          json.loads(cookies))
            account = compose(
                "exec", "-T", "cloud", "curl", "--silent", "--show-error", "--cacert", "/tls/test-ca.pem",
                "--write-out", "%{http_code}", "--output", "/dev/null",
                "--cookie", f"__Host-_rt={login['value']}", "https://pontia.dev/settings/account",
            ).stdout
            self.assertEqual(account, b"200")
            with self.assertRaises(subprocess.CalledProcessError):
                compose("exec", "-T", "device", "codex", "login", "status")
            compose("exec", "-T", "device", "touch", "/home/e2e/workspace/discard-me")
            compose("exec", "-T", "cloud", "./node_modules/.bin/wrangler", "d1", "execute", "DB",
                    "--local", "--persist-to", "/state", "--command",
                    "INSERT INTO users (id) VALUES ('discard-me');")
            # Stopping Cloud must leave its domain pinned, not resolve production.
            compose("stop", "cloud")
            hosts = compose("exec", "-T", "device", "getent", "hosts", "pontia.dev").stdout
            subnet = json.loads((first.state / "network.json").read_text())["subnet"]
            self.assertEqual(hosts.split()[0].decode(), str(ipaddress.ip_network(subnet)[2]))
            with self.assertRaises(subprocess.CalledProcessError):
                compose("exec", "-T", "device", "curl", "--noproxy", "*", "--max-time", "3",
                        "--fail", "https://pontia.dev/")
            compose("exec", "-T", "device", "python3", "-c",
                    "from pathlib import Path; "
                    "Path('/home/e2e/.claude/.credentials.json').write_text('{\"test_credential\":\"refreshed\"}'); "
                    "Path('/home/e2e/.claude/settings.json').write_text('{\"permissions\":{\"allow\":[\"Bash\"]}}'); "
                    "Path('/home/e2e/.claude/projects').mkdir(exist_ok=True); "
                    "Path('/home/e2e/.claude/projects/discard-me').touch()")
            first.down()
            for resource in ("container", "volume", "network"):
                self.assertFalse(first.resources(resource))
            second.compose(["exec", "-T", "device", "test", "-f", "/home/e2e/workspace/keep-me"])
            second.compose(["exec", "-T", "device", "test", "!", "-e", "/home/e2e/workspace/discard-me"])
            second.compose(["exec", "-T", "cloud", "curl", "--fail", "--silent", "--cacert",
                            "/e2e-test-ca.pem", "https://pontia.dev/"], capture_output=True)
            saved = first.state / "auth/device/claude.json"
            self.assertEqual(json.loads(saved.read_text()), {"test_credential": "refreshed"})
            self.assertEqual(saved.stat().st_mode & 0o777, 0o600)
            self.assertEqual(json.loads((state / "auth/device/claude.json").read_text()),
                             {"test_credential": "original"})
            self.assertEqual(json.loads((first.state / "auth/device/claude-settings.json").read_text()), claude_settings)
            (state / "openai-api-key").write_text("sk-fixture-openai-test-key\n")
            first.up()
            compose("exec", "-T", "device", "test", "!", "-e", "/home/e2e/workspace/discard-me")
            auth = compose("exec", "-T", "device", "codex", "login", "status")
            self.assertIn(b"API key", auth.stdout + auth.stderr)
            imported_pi = compose("exec", "-T", "device", "cat", "/home/e2e/.pi/agent/auth.json")
            self.assertEqual(json.loads(imported_pi.stdout), {
                "openai": {"type": "api_key", "key": "sk-fixture-openai-test-key"},
                "openrouter": pi_auth["openrouter"],
            })
            self.assertEqual(json.loads((state / "auth/device/pi.json").read_text()), pi_auth)
            credentials = compose("exec", "-T", "device", "sh", "-c",
                                  "test ! -e /home/e2e/.claude/projects/discard-me && "
                                  "cat /home/e2e/.claude/.credentials.json")
            self.assertEqual(json.loads(credentials.stdout), {"test_credential": "refreshed"})
            settings = compose("exec", "-T", "device", "cat", "/home/e2e/.claude/settings.json")
            native = json.loads(settings.stdout)
            self.assertEqual(native["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8317")
            self.assertEqual(native["model"], "gpt-4o-mini")
            self.assertNotIn(b"sk-fixture-openai-test-key", settings.stdout)
            models = compose("exec", "-T", "device", "python3", "-c",
                             "import json,urllib.request; from pathlib import Path; "
                             "s=json.loads(Path('/home/e2e/.claude/settings.json').read_text()); "
                             "r=urllib.request.Request(s['env']['ANTHROPIC_BASE_URL']+'/v1/models', "
                             "headers={'Authorization':'Bearer '+s['env']['ANTHROPIC_AUTH_TOKEN']}); "
                             "print(urllib.request.urlopen(r).read().decode())")
            self.assertIn("gpt-4o-mini", [model["id"] for model in json.loads(models.stdout)["data"]])
            compose("exec", "-T", "device", "test", "!", "-e", "/run/claude-proxy/auth/kv.db")
            fresh = compose("exec", "-T", "cloud", "./node_modules/.bin/wrangler", "d1", "execute", "DB",
                            "--local", "--persist-to", "/state", "--json", "--command",
                            "SELECT count(*) AS count FROM users WHERE id='discard-me';")
            self.assertEqual(json.loads(fresh.stdout)[0]["results"][0]["count"], 0)
        finally:
            for lab in (first, second):
                lab.down()
                for resource in ("container", "volume", "network"):
                    self.assertFalse(lab.resources(resource))

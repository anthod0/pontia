import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class ClaudeProxyTests(unittest.TestCase):
    def setUp(self):
        sandbox = tempfile.TemporaryDirectory(prefix="pontia-claude-proxy-test-")
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name)
        self.settings = self.root / "settings.json"
        self.key = self.root / "openai-api-key"
        self.runtime = self.root / "runtime"

    def configure(self):
        result = subprocess.run([
            sys.executable, str(Path(__file__).with_name("configure-claude-proxy.py")),
            str(self.settings), str(self.key), str(self.runtime),
        ], check=True, capture_output=True)
        self.assertEqual(result.stdout + result.stderr, b"")

    def test_openai_key_overrides_saved_provider_without_exposing_upstream_key_to_claude(self):
        self.settings.write_text(json.dumps({
            "model": "previous-model", "apiKeyHelper": "old-helper",
            "permissions": {"allow": ["Read"]},
            "env": {"ANTHROPIC_BASE_URL": "https://old-provider.example",
                    "ANTHROPIC_API_KEY": "old-key", "CUSTOM_SETTING": "keep"},
        }))
        for key in ("first-fixture", "rotated-fixture"):
            self.key.write_text(key + "\n")
            self.configure()
            settings = json.loads(self.settings.read_text())
            config = json.loads((self.runtime / "config.yaml").read_text())
            self.assertEqual(settings["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8317")
            self.assertEqual(settings["env"]["ANTHROPIC_API_KEY"], "")
            self.assertEqual(settings["model"], "gpt-4o-mini")
            self.assertEqual(settings["permissions"], {"allow": ["Read"]})
            self.assertEqual(settings["env"]["CUSTOM_SETTING"], "keep")
            self.assertNotIn("apiKeyHelper", settings)
            self.assertNotIn(key, self.settings.read_text())
            self.assertEqual(config["access"]["api-keys"], [settings["env"]["ANTHROPIC_AUTH_TOKEN"]])
            self.assertEqual(config["api-keys"]["openai-compatibility"][0]["keys"], [{"api-key": key}])
            self.assertTrue(config["server"]["kv-store"]["disabled"])
            for path in (self.settings, self.runtime / "config.yaml"):
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_key_configures_claude_without_existing_settings(self):
        self.key.write_text("fixture-key")
        self.configure()
        self.assertEqual(json.loads(self.settings.read_text())["model"], "gpt-4o-mini")

    def test_empty_key_preserves_native_auth_and_does_not_start_gateway(self):
        for key in ("", "\n"):
            self.key.write_text(key)
            self.configure()
            self.assertFalse(self.runtime.exists())
            self.assertFalse(self.settings.exists())
            original = '{"env":{"ANTHROPIC_API_KEY":"native-key"}}\n'
            self.settings.write_text(original)
            self.configure()
            self.assertEqual(self.settings.read_text(), original)
            self.settings.unlink()

from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import tomllib
import unittest


class GrokConfigTests(unittest.TestCase):
    def setUp(self):
        root = tempfile.TemporaryDirectory(prefix="pontia-grok-test-")
        self.addCleanup(root.cleanup)
        self.key = Path(root.name) / "test secret"
        self.config = Path(root.name) / ".grok/config.toml"

    def configure(self):
        result = subprocess.run([
            sys.executable, str(Path(__file__).with_name("configure-grok.py")),
            str(self.config), str(self.key),
        ], capture_output=True, check=True)
        self.assertEqual(result.stdout + result.stderr, b"")

    def test_default_model_uses_current_mounted_key(self):
        self.key.write_text("first-fixture\n")
        self.configure()
        text = self.config.read_text()
        self.assertNotIn("first-fixture", text)
        settings = tomllib.loads(text)
        model = settings["model"][settings["models"]["default"]]
        self.assertEqual(model["model"], "gpt-4o-mini")
        self.assertEqual(model["base_url"], "https://api.openai.com/v1")
        self.assertEqual(model["api_backend"], "chat_completions")
        command = settings["auth_provider"][model["auth_provider"]]["command"]
        self.key.write_text("rotated-fixture\n")
        self.assertEqual(subprocess.check_output(shlex.split(command)), b"rotated-fixture\n")

    def test_empty_secret_skips_openai_configuration(self):
        self.key.write_text("\n")
        self.configure()
        self.assertFalse(self.config.exists())

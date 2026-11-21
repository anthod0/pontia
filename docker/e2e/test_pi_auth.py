import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class PiAuthTests(unittest.TestCase):
    def setUp(self):
        sandbox = tempfile.TemporaryDirectory(prefix="pontia-pi-auth-test-")
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name)
        self.auth = self.root / "auth.json"
        self.key = self.root / "openai-api-key"

    def configure(self):
        result = subprocess.run([
            sys.executable, str(Path(__file__).with_name("configure-pi-auth.py")),
            str(self.auth), str(self.key),
        ], check=True, capture_output=True)
        self.assertEqual(result.stdout + result.stderr, b"")

    def test_key_replaces_oauth_preserves_other_providers_and_rotates(self):
        original = {
            "openai": {"type": "oauth", "refresh": "expired"},
            "openrouter": {"type": "api_key", "key": "router-fixture"},
        }
        self.auth.write_text(json.dumps(original))
        self.auth.chmod(0o644)
        for key in ("first-fixture", "rotated-fixture"):
            with self.subTest(key=key):
                self.key.write_text(key + "\n")
                self.configure()
                self.assertEqual(json.loads(self.auth.read_text()), {
                    "openai": {"type": "api_key", "key": key},
                    "openrouter": original["openrouter"],
                })
                self.assertEqual(self.auth.stat().st_mode & 0o777, 0o600)

    def test_key_creates_private_credentials_without_existing_login(self):
        self.key.write_text("fixture-key\n")
        self.configure()
        self.assertEqual(json.loads(self.auth.read_text()), {
            "openai": {"type": "api_key", "key": "fixture-key"},
        })
        self.assertEqual(self.auth.stat().st_mode & 0o777, 0o600)

    def test_empty_key_preserves_credentials_or_their_absence(self):
        for empty in ("", "\n"):
            with self.subTest(key=empty):
                self.key.write_text(empty)
                self.configure()
                self.assertFalse(self.auth.exists())
                original = b'{"openai": {"type": "oauth", "refresh": "existing"}}\n'
                self.auth.write_bytes(original)
                self.configure()
                self.assertEqual(self.auth.read_bytes(), original)
                self.auth.unlink()

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "release-version.py"


class ReleaseVersionTest(unittest.TestCase):
    def run_script(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_repository_manifests_use_placeholders(self) -> None:
        result = self.run_script("check")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_materializes_product_version_only_in_staged_package(self) -> None:
        with tempfile.TemporaryDirectory(prefix="pontia-release-version-") as directory:
            package_json = Path(directory) / "package.json"
            package_json.write_text(json.dumps({"name": "example", "version": "0.0.0"}))

            result = self.run_script("materialize-npm", str(package_json))

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                json.loads(package_json.read_text())["version"],
                (ROOT / "VERSION").read_text().strip(),
            )

    def test_rejects_materializing_a_non_placeholder_package(self) -> None:
        with tempfile.TemporaryDirectory(prefix="pontia-release-version-") as directory:
            package_json = Path(directory) / "package.json"
            package_json.write_text(json.dumps({"name": "example", "version": "1.2.3"}))

            result = self.run_script("materialize-npm", str(package_json))

            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(json.loads(package_json.read_text())["version"], "1.2.3")


if __name__ == "__main__":
    unittest.main()

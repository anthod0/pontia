"""Apply the E2E OpenAI API key to the container's Pi credentials."""

import json
from pathlib import Path
import sys


def configure(auth_path, key_path):
    key = key_path.read_text().strip()
    if not key:
        return
    credentials = json.loads(auth_path.read_text()) if auth_path.exists() else {}
    credentials["openai"] = {"type": "api_key", "key": key}
    auth_path.touch(mode=0o600, exist_ok=True)
    auth_path.chmod(0o600)
    auth_path.write_text(json.dumps(credentials) + "\n")


if __name__ == "__main__":
    configure(Path(sys.argv[1]), Path(sys.argv[2]))

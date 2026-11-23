"""Configure Grok's OpenAI model for the disposable device."""

import json
from pathlib import Path
import shlex
import sys


def configure(config, key):
    if not key.read_bytes().strip():
        return
    config.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    command = json.dumps(shlex.join(["cat", str(key)]))
    config.write_text(f'''[models]
default = "gpt-4o-mini"
session_summary = "gpt-4o-mini"

[model.gpt-4o-mini]
model = "gpt-4o-mini"
base_url = "https://api.openai.com/v1"
api_backend = "chat_completions"
context_window = 128000
auth_provider = "openai"

[auth_provider.openai]
command = {command}
''')


if __name__ == "__main__":
    configure(*(Path(arg) for arg in sys.argv[1:]))

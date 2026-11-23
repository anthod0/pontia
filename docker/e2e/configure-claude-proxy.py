"""Configure the disposable Claude-to-OpenAI gateway and native Claude settings."""

import json
from pathlib import Path
import secrets
import sys
import uuid


def write_private(path, value):
    path.touch(mode=0o600, exist_ok=True)
    path.chmod(0o600)
    path.write_text(json.dumps(value) + "\n")


def configure(settings_path, key_path, runtime):
    key = key_path.read_text().strip()
    if not key:
        return
    runtime.mkdir(mode=0o700, parents=True, exist_ok=True)
    token = secrets.token_urlsafe(32)
    model = "gpt-4o-mini"
    write_private(runtime / "config.yaml", {
        "config-version": 8,
        "server": {
            "host": "127.0.0.1", "port": 8317, "server-id": str(uuid.uuid4()),
            "commercial-mode": True, "kv-store": {"disabled": True},
        },
        "management": {"secret-key": "", "disable-control-panel": True},
        "access": {"api-keys": [token]},
        "oauth": {"auth-dir": str(runtime / "auth")},
        "routing": {"retry": {"request-retry": 0}},
        "requests": {"payload": {"filter": [{
            "models": [{"name": model, "protocol": "openai"}],
            "params": ["reasoning_effort"],
        }]}},
        "api-keys": {"openai-compatibility": [{
            "name": "openai", "base-url": "https://api.openai.com/v1",
            "keys": [{"api-key": key}], "models": [{"name": model}],
        }]},
        "observability": {"logs": {"logging-to-file": False, "request-log": False}},
    })
    settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
    settings.update(model=model, alwaysThinkingEnabled=False)
    settings.pop("apiKeyHelper", None)
    settings.setdefault("env", {}).update({
        "ANTHROPIC_BASE_URL": "http://127.0.0.1:8317",
        "ANTHROPIC_AUTH_TOKEN": token,
        "ANTHROPIC_API_KEY": "",
        "ANTHROPIC_MODEL": model,
        "ANTHROPIC_DEFAULT_SONNET_MODEL": model,
        "ANTHROPIC_DEFAULT_OPUS_MODEL": model,
        "ANTHROPIC_DEFAULT_HAIKU_MODEL": model,
        "CLAUDE_CODE_SUBAGENT_MODEL": model,
        "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "4096",
        "MAX_THINKING_TOKENS": "0",
    })
    write_private(settings_path, settings)


if __name__ == "__main__":
    configure(*(Path(arg) for arg in sys.argv[1:]))

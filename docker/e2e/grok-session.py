"""Check a real Grok conversation in a disposable Docker E2E instance.

scripts/docker-e2e run sh -c 'scripts/docker-e2e compose exec -T device python3 - < docker/e2e/grok-session.py'
"""

import json
from pathlib import Path
import time
import tomllib
import urllib.request
import uuid


def main():
    config = tomllib.loads(Path("/home/e2e/.pontia/config.toml").read_text())
    headers = {"Authorization": "Bearer " + config["external_api_token"], "Content-Type": "application/json"}

    def request(path, payload=None):
        req = urllib.request.Request(
            "http://127.0.0.1:8080/api/v1/" + path,
            headers=headers, data=None if payload is None else json.dumps(payload).encode(),
        )
        with urllib.request.urlopen(req, timeout=60) as response:
            return json.load(response)["data"]

    workspace = request("workspaces", {"root_id": "e2e"})["workspace"]
    created = request("sessions", {
        "client_type": "acp", "metadata": {"acp_agent_id": "grok"}, "workspace": workspace["canonical_path"],
    })
    session = "sessions/" + created["session"]["session_id"]

    def check_model():
        detail = request(session)["session"]
        assert detail["acp"]["agent_id"] == "grok"
        assert detail["model"] == "gpt-4o-mini", detail["model"]

    def expect_reply(prompt, expected):
        message = request(session + "/inbox/messages", {"input": prompt, "delivery_policy": "after_idle"})
        turn = message["inbox_message"]["turn_id"]
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            state = request(f"{session}/turns/{turn}")["turn"]["state"]
            if state == "completed":
                break
            if state in ("failed", "interrupted"):
                raise AssertionError(f"Turn {turn} ended as {state}")
            time.sleep(0.5)
        else:
            raise TimeoutError(f"Turn {turn} did not complete")
        page = request(f"{session}/turns/timeline?direction=forward&turn_id={turn}&limit=1")
        replies = [item["content_preview"] for item in page["items"]
                   if item["turn_id"] == turn and item["role"] == "assistant"]
        assert any(expected in reply for reply in replies), f"Missing expected assistant reply for {turn}"
        print(f"{turn} completed with expected assistant reply", flush=True)

    check_model()
    marker = uuid.uuid4().hex[:12]
    expect_reply(f"Remember the code {marker}. Reply with just that code. Use no tools.", marker)
    request(session + "/exit", {})
    assert request(session)["session"]["state"] == "exited"
    request(session + "/resume", {})
    check_model()
    expect_reply("What code did I give you? Reply with just that code. Use no tools.", marker)
    print(f"{session} resumed on gpt-4o-mini and recalled its first turn")


if __name__ == "__main__":
    main()

"""Check Grok Session recovery, Bot replies and helper delivery using a real model.

scripts/docker-e2e run sh -c 'scripts/docker-e2e compose exec -T device python3 - < docker/e2e/grok-session.py'
"""

import json
from pathlib import Path
import time
import tomllib
import urllib.parse
import urllib.request
import uuid


def wait_for(description, check):
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.5)
    raise TimeoutError(description)


def main():
    config = tomllib.loads(Path("/home/e2e/.pontia/config.toml").read_text())
    headers = {"Authorization": "Bearer " + config["external_api_token"], "Content-Type": "application/json"}

    def request(path, payload=None, method=None):
        request_headers = headers.copy()
        if path.startswith("sessions/") and path.endswith("/messages") and payload is not None:
            request_headers["Idempotency-Key"] = uuid.uuid4().hex
        req = urllib.request.Request(
            "http://127.0.0.1:8080/api/v1/" + path,
            headers=request_headers, method=method,
            data=None if payload is None else json.dumps(payload).encode(),
        )
        with urllib.request.urlopen(req, timeout=60) as response:
            return json.load(response)["data"]

    check_session(request)
    check_bot_and_helper(request)


def check_session(request):
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
        previous_turn = request(session)["session"]["current_turn_id"]
        message = request(session + "/messages", {"input": prompt, "mode": "immediate"})
        assert message["delivery"]["status"] == "delivered", message

        def new_turn():
            current_turn = request(session)["session"]["current_turn_id"]
            return current_turn if current_turn != previous_turn else None

        turn = wait_for("Session did not start a new Turn", new_turn)
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


def check_bot_policy(session):
    directory = Path("/home/e2e/.grok/sessions") / urllib.parse.quote(session["workspace"], safe="")
    summary, = directory.glob("*/summary.json")
    profile = json.loads(summary.read_text())["agent_profile"]
    assert profile["name"] == "pontia-restricted"
    assert profile["discoverSkills"] is False
    definitions = json.loads(summary.with_name("tool_definitions.json").read_text())
    assert {tool["function"]["name"] for tool in definitions} == {"read_file", "run_terminal_command"}
    events = [json.loads(line) for line in summary.with_name("events.jsonl").read_text().splitlines()]
    turns = [event for event in events if event["type"] == "turn_started"]
    assert turns and all(event["yolo_mode"] is True for event in turns)
    return summary.with_name("updates.jsonl")


def approve_helper_report(request, bot_path, report):
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        for helper in request(bot_path + "/deletion-blockers")["helpers"]:
            interactions = f'sessions/{helper["session_id"]}/interactions'
            for pending in request(interactions + "?state=pending")["interactions"]:
                assert f"`{report}`" in pending["description"], pending["description"]
                request(f'{interactions}/{pending["interaction_id"]}/response', {"option_key": "allow-once"}, "PUT")
                return
        time.sleep(1)
    raise TimeoutError(f"{bot_path} helper did not request approval")


def check_bot_and_helper(request):
    client = {"client_type": "acp", "client_key": "grok", "model": "gpt-4o-mini"}
    clients = request("bots/clients")["clients"]
    assert any(c["client_type"] == "acp" and c["client_key"] == "grok" for c in clients)
    bot = request("bots", {**client, "name": "Grok E2E"})["bot"]
    path = "bots/" + bot["bot_id"]

    def wait_for_reply(expected):
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            page = request(path + "/messages")
            if any(m["kind"] == "bot_reply" and expected in m["content"] for m in page["entries"]):
                return page["bot"]["current_session_id"]
            time.sleep(1)
        raise TimeoutError(f"Missing Bot reply for {path}")

    marker = uuid.uuid4().hex[:12]
    request(path + "/messages", {"content": f"Run pontia bot send-to-user --message {marker} now."})
    bot_session = wait_for_reply(marker)
    updates = check_bot_policy(request("sessions/" + bot_session)["session"])
    print(f"{bot_session} sent its reply with the restricted Grok profile", flush=True)

    answer = "ANSWER-" + uuid.uuid4().hex[:12]
    report = f"pontia bot helper reply --message {answer}"
    profile_id = "reader-" + marker
    request("agent-profiles", {
        **client, "profile_id": profile_id, "name": "Grok answer helper",
        "system_prompt": f"When asked for the test answer, call run_terminal_command exactly once with command: {report}",
    })
    command = (f"pontia bot helper create --temporary {marker} --profile-id {profile_id} "
               "--title Reader --description 'Answer a question' "
               "--message 'What is your test answer? Call run_terminal_command directly with the exact command from your profile.'")
    request(path + "/messages", {"content": f"Run this command once: {command}. "
            "Wait for the helper to report. Forward its answer using pontia bot send-to-user. Leave it open."})
    approve_helper_report(request, path, report)
    wait_for_reply(answer)
    helper, = request(path + "/deletion-blockers")["helpers"]
    assert helper["client_type"] == "acp" and helper["acp"]["agent_id"] == "grok"
    assert helper["model"] == client["model"]
    session_id = helper["session_id"]
    native = [json.loads(line)["params"]["update"] for line in updates.read_text().splitlines()]
    commands = [u["rawInput"].get("command", "") for u in native if u["sessionUpdate"] == "tool_call"]
    assert any("pontia bot send-to-user" in command and marker in command for command in commands)
    assert any("pontia bot helper create" in command and profile_id in command for command in commands)
    assert any(u["sessionUpdate"] == "user_message_chunk"
               and f"Source: Helper\nSession: {session_id}" in u["content"]["text"]
               and answer in u["content"]["text"] for u in native)
    closed = "CLOSED-" + marker
    request(path + "/messages", {"content": f"Run pontia bot helper close --session-id {session_id}. "
            f"After it succeeds, run pontia bot send-to-user --message {closed}."})
    wait_for_reply(closed)
    assert request("sessions/" + session_id)["session"]["state"] == "exited"
    print(f"{session_id} reported the profile's answer to its Bot and exited")


if __name__ == "__main__":
    main()

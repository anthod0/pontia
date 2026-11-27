"""Exercise a real Claude Code Bot, profile helper and single-node Workflow.

scripts/docker-e2e run sh -c 'scripts/docker-e2e compose exec -T device python3 - < docker/e2e/claude-session.py'
"""

import json
import shlex
from pathlib import Path
import time
import tomllib
import urllib.request
import uuid


CLIENT = {"client_type": "claude-agent", "client_key": "default", "model": "gpt-4o-mini"}
DENIED = {
    "Edit", "Write", "NotebookEdit", "Agent", "Task", "SendMessage", "TeamCreate", "TeamDelete",
    "Workflow", "EnterWorktree", "ExitWorktree", "CronCreate", "CronDelete", "ScheduleWakeup",
    "RemoteTrigger", "Monitor",
}


def wait_for(description, check):
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.5)
    raise TimeoutError(description)


def bash_task(command):
    return ("Use the Bash tool exactly once. Set its command field to this literal shell command "
            "without echoing it:\n" + command + "\nAfter the tool returns, end this turn.")


def transcript(request, session_id):
    events = request(f"sessions/{session_id}/events")["events"]
    native = next(e["payload"]["client_session_key"] for e in events if e["type"] == "session.ready")
    path, = Path("/home/e2e/.claude/projects").glob(f"*/{native}.jsonl")
    return [json.loads(line) for line in path.read_text().splitlines()]


def commands(records):
    return [block["input"]["command"] for record in records if record["type"] == "assistant"
            for block in record["message"]["content"]
            if block["type"] == "tool_use" and block["name"] == "Bash"]


def check_policy(session, records):
    shapes = [record["shape"]["tools"] for record in records if record["type"] == "api-request-shape"]
    assert shapes and all(any(tool["name"] == "Bash" for tool in shape) for shape in shapes)
    tools = {tool["name"] for shape in shapes for tool in shape}
    for record in records:
        delta = record.get("attachment", {})
        if delta.get("type") == "deferred_tools_delta":
            tools.update(delta["addedNames"])
    assert tools.isdisjoint(DENIED | {"Skill", "AskUserQuestion"}), tools
    assert not any(name.startswith("mcp__") for name in tools), tools

    runtime, = session["runtimes"]
    pid = json.loads(runtime["process_fingerprint"])["pid"]
    children = Path(f"/proc/{pid}/task/{pid}/children").read_text().split()
    child_args = (Path(f"/proc/{child}/cmdline").read_bytes().decode().split("\0") for child in children)
    argv, = (args for args in child_args if "--output-format" in args)
    assert "--disable-slash-commands" in argv and "--strict-mcp-config" in argv
    assert any("mcp__*" in arg.split("=", 1)[1].split(",") for arg in argv if arg.startswith("--disallowedTools="))
    assert "--permission-mode=bypassPermissions" in argv
    print(f'{session["session_id"]} tools={sorted(tools)}, skills off, strict MCP, bypassPermissions', flush=True)


def check_bot_and_helper(request, marker):
    clients = request("bots/clients")["clients"]
    assert any(c["client_type"] == CLIENT["client_type"] and c["client_key"] == CLIENT["client_key"]
               for c in clients)
    answer = "ANSWER-" + uuid.uuid4().hex[:12]
    profile = "reader-" + marker
    report = f"pontia bot helper reply --message {answer}"
    request("agent-profiles", {**CLIENT, "profile_id": profile, "name": "Claude answer helper",
                               "system_prompt": "When asked for the test answer, " + bash_task(report)})
    bot = request("bots", {**CLIENT, "name": "Claude E2E"})["bot"]
    path = "bots/" + bot["bot_id"]

    def send(content):
        request(path + "/messages", {"content": content})

    def reply(expected):
        def check():
            page = request(path + "/messages")
            if any(m["kind"] == "bot_reply" and expected in m["content"] for m in page["entries"]):
                return page["bot"]["current_session_id"]
        return wait_for(f"{path} did not send {expected}", check)

    first = f"pontia bot send-to-user --message {marker}"
    send(bash_task(first))
    bot_session_id = reply(marker)

    def idle():
        session = request("sessions/" + bot_session_id)["session"]
        assert not session["claude-agent"]["unavailable_reason"], session["claude-agent"]
        return session if session["state"] == "idle" else None

    bot_session = wait_for(f"Bot Session {bot_session_id} did not finish its turn", idle)
    assert bot_session["client_type"] == CLIENT["client_type"] and bot_session["model"] == CLIENT["model"]
    records = transcript(request, bot_session_id)
    assert any(shlex.split(command) == shlex.split(first) for command in commands(records))
    check_policy(bot_session, records)
    print(f"{bot_session_id} ran {first} and delivered the Bot reply on {bot_session['model']}", flush=True)

    create = (f"pontia bot helper create --temporary {marker} --profile-id {profile} "
              "--title Reader --description 'Answer a question' "
              "--message 'What is your test answer? Execute the exact Bash command in your profile.'")
    send(bash_task(create) + "\nWhen the helper reports, use Bash to run "
         "pontia bot send-to-user --message followed by its answer. Keep the helper open.")
    helper, = wait_for("Bot did not create the helper",
                       lambda: request(path + "/deletion-blockers")["helpers"])
    session_id = helper["session_id"]
    interactions = f"sessions/{session_id}/interactions"
    pending, = wait_for("Helper did not request approval",
                        lambda: request(interactions + "?state=pending")["interactions"])
    assert pending["description"].splitlines()[-1] == report, pending["description"]
    request(f'{interactions}/{pending["interaction_id"]}/response', {"option_key": "allow"}, "PUT")
    reply(answer)
    detail = request("sessions/" + session_id)["session"]
    assert detail["client_type"] == CLIENT["client_type"] and detail["model"] == CLIENT["model"]
    assert any(shlex.split(command) == shlex.split(report) for command in commands(transcript(request, session_id)))
    records = transcript(request, bot_session_id)
    assert any(profile in command and "pontia bot helper create" in command for command in commands(records))
    assert any(record["type"] == "user" and isinstance(content := record["message"]["content"], str)
               and f"Source: Helper\nSession: {session_id}" in content and answer in content for record in records)
    assert any("pontia bot send-to-user" in command and answer in command for command in commands(records))
    wait_for(f"Bot Session {bot_session_id} did not finish forwarding", idle)
    close = f"pontia bot helper close --session-id {session_id}"
    send(bash_task(close))
    wait_for(f"helper {session_id} did not exit",
             lambda: request("sessions/" + session_id)["session"]["state"] == "exited")
    assert any(shlex.split(command) == shlex.split(close) for command in commands(transcript(request, bot_session_id)))
    assert not request(f"sessions/{bot_session_id}/interactions")["interactions"]
    print(f'{session_id} reported {answer}, Bot forwarded the answer and closed the helper', flush=True)


def check_workflow(request, marker):
    profile = "writer-" + marker
    request("agent-profiles", {**CLIENT, "profile_id": profile, "name": "Claude Workflow writer",
                               "system_prompt": "Follow the node instructions and submit the assigned output."})
    workspace = request("workspaces", {"root_id": "e2e"})["workspace"]
    directory = Path(workspace["canonical_path"])
    definition = directory / f"workflow-{marker}.toml"
    result = directory / f"workflow-{marker}.id"
    command = f'printf "%s" {marker} > "/home/e2e/.pontia/workflows/$PONTIA_WORKFLOW_ID/handoff/result.md" && pontia workflow submit'
    definition.write_text(
        f'title = "Claude single node"\ncwd = "."\n[[nodes]]\ntype = "agent"\n'
        f'phase = "Test"\ntitle = "Write result"\noutput = "result.md"\n'
        f'execution_profile_id = "{profile}"\ninstructions = {json.dumps(bash_task(command))}\n'
    )
    launch_command = f"pontia workflow run {shlex.quote(str(definition))} > {shlex.quote(str(result))}"
    origin = request("sessions", {
        "client_type": CLIENT["client_type"], "model": CLIENT["model"],
        "workspace": str(directory), "mode": "yolo",
        "initial_task": {"input": bash_task(launch_command)},
    })["session"]["session_id"]
    workflow = wait_for("initiating Session did not start the Workflow",
                        lambda: result.read_text().strip() if result.exists() else None)
    handoff = Path("/home/e2e/.pontia/workflows") / workflow / "handoff/result.md"
    session_id = request("workflows/" + workflow)["workflow"]["nodes"][0]["session_id"]
    session = "sessions/" + session_id

    def completed():
        detail = request("workflows/" + workflow)["workflow"]
        if detail["state"] == "completed":
            return detail
        assert detail["state"] not in ("failed", "idle"), (workflow, detail["state"], detail["failure_message"])
        for interaction in request(session + "/interactions?state=pending")["interactions"]:
            summary = interaction["description"]
            assert str(handoff) in summary or "pontia workflow submit" in summary, summary
            request(f'{session}/interactions/{interaction["interaction_id"]}/response', {"option_key": "allow"}, "PUT")

    detail = wait_for(f"{workflow} did not complete", completed)
    node, = detail["nodes"]
    assert node["session_id"] == session_id and detail["initiator_session_id"] == origin and node["submitted_at"]
    actual = request(session)["session"]
    assert actual["client_type"] == CLIENT["client_type"] and actual["model"] == CLIENT["model"]
    runtime, = actual["runtimes"]
    assert runtime["role"] == "claude_sdk_host"
    assert handoff.read_text().strip() == marker

    def report():
        return next((record for record in transcript(request, origin)
                     if record["type"] == "user" and str(handoff) in json.dumps(record)), None)

    wait_for("initiating Session did not receive output paths", report)
    print(f'{workflow} completed, {session_id} submitted {marker} via {runtime["role"]} {runtime["runtime_id"]}',
          flush=True)


def main():
    config = tomllib.loads(Path("/home/e2e/.pontia/config.toml").read_text())
    headers = {"Authorization": "Bearer " + config["external_api_token"], "Content-Type": "application/json"}

    def request(path, payload=None, method=None):
        req = urllib.request.Request("http://127.0.0.1:8080/api/v1/" + path, headers=headers, method=method,
                                     data=None if payload is None else json.dumps(payload).encode())
        with urllib.request.urlopen(req, timeout=60) as response:
            return json.load(response)["data"]

    marker = uuid.uuid4().hex[:12]
    check_workflow(request, marker)
    check_bot_and_helper(request, marker)


if __name__ == "__main__":
    main()

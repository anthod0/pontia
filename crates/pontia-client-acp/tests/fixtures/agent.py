"""Controlled stdio peer. All files live beneath the caller's temporary root."""
import json
import os
from pathlib import Path
import sys
import threading
import time

root = Path(os.environ["ACP_FIXTURE_ROOT"])
lock = threading.Lock()
active = None
number = 0


def send(message):
    with lock:
        print(json.dumps({"jsonrpc": "2.0", **message}), flush=True)


def log(message):
    with lock:
        with (root / "wire.jsonl").open("a") as output:
            output.write(json.dumps(message) + "\n")


def response(request, result):
    send({"id": request["id"], "result": result})


def wait_for(name):
    while not (root / name).exists():
        time.sleep(0.005)


def prompt(request, ordinal, cancellation):
    global active
    wait_for(f"update-{ordinal}")
    if os.environ.get("ACP_PERMISSION") == "1":
        options = [
            {"optionId": "once:原", "name": "Allow once", "kind": "allow_once"},
            {"optionId": "always:原", "name": "Allow always", "kind": "allow_always"},
            {"optionId": "reject-once", "name": "Reject once", "kind": "reject_once"},
            {"optionId": "reject-always", "name": "Reject always", "kind": "reject_always"},
        ]
        if os.environ.get("ACP_BAD_OPTIONS") == "1":
            options.append(options[0])
        request_ids = [700 + ordinal, f"permission-{ordinal}", f"unanswered-{ordinal}"]
        if os.environ.get("ACP_REPLY_BACKPRESSURE") == "1":
            options = [{"optionId": "x" * 262144, "name": "Allow once", "kind": "allow_once"}]
            request_ids = [700 + ordinal]
        for request_id in request_ids:
            send({"id": request_id, "method": "session/request_permission", "params": {
                "sessionId": "other-session" if os.environ.get("ACP_WRONG_SESSION") == "1" else "native-id",
                "toolCall": {"toolCallId": "same-tool", "title": f"<script>untrusted</script> request:{request_id}", "rawInput": {"secret": "do-not-persist"}},
                "options": options,
            }})
    if os.environ.get("ACP_NATIVE_CANCEL") == "1":
        wait_for(f"native-cancel-{ordinal}")
        send({"method": "$/cancel_request", "params": {"requestId": f"unanswered-{ordinal}"}})
    if os.environ.get("ACP_EXTENSION_UPDATE") == "1":
        send({"method": "session/update", "params": {
            "sessionId": "native-id", "update": {"sessionUpdate": "extension_running"}
        }})
        send({"id": "extension-barrier", "method": "_fixture_barrier", "params": {}})
    if os.environ.get("ACP_NO_UPDATES") != "1":
        send({"method": "session/update", "params": {
            "sessionId": "native-id",
            "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "streamed answer"}}
        }})
    if os.environ.get("ACP_REPLY_BACKPRESSURE") == "1":
        wait_for(f"backpressure-{ordinal}")
        # The main thread cannot drain stdin until these updates have all been
        # written. A response writer holding the reader's state lock deadlocks.
        for _ in range(64):
            send({"method": "session/update", "params": {
                "sessionId": "native-id", "update": {
                    "sessionUpdate": "tool_call_update", "toolCallId": "pressure", "rawInput": "x" * 4096
                }
            }})
        (root / f"drain-input-{ordinal}").touch()
    wait_for(f"release-{ordinal}")
    if os.environ.get("ACP_DROP_PROMPT") == "1":
        os._exit(0)
    reason = "cancelled" if cancellation.is_set() else request["params"]["prompt"][0]["text"]
    active = None
    response(request, {"stopReason": reason})
    if os.environ.get("ACP_EXIT_AFTER_RESULT") == "1":
        os._exit(0)


log({"startup": {"cwd": os.getcwd(), "executable": sys.executable, "argv": sys.argv[1:], "literal": os.environ.get("ACP_LITERAL")}})
for line in sys.stdin:
    request = json.loads(line)
    log(request)
    method = request.get("method")
    if method is None:
        continue
    if method == "initialize":
        caps = {"loadSession": os.environ.get("ACP_NO_LOAD") != "1", "sessionCapabilities": {}}
        if os.environ.get("ACP_NO_CLOSE") != "1":
            caps["sessionCapabilities"]["close"] = {}
        response(request, {"protocolVersion": 1, "agentCapabilities": caps,
                           "agentInfo": {"name": "fixture", "title": "Test Agent", "version": "arbitrary-release", "_meta": {"build": "fixture-build"}}})
    elif method == "session/new":
        if os.environ.get("ACP_HOLD_NEW") == "1":
            wait_for("release-new")
        response(request, {"sessionId": "native-id"})
        if os.environ.get("ACP_BREAK_INPUT") == "1":
            sys.stdin.read(1)
            os._exit(0)
    elif method == "session/prompt":
        if active is not None:
            log({"overlap": True})
        number += 1
        active = threading.Event()
        threading.Thread(target=prompt, args=(request, number, active), daemon=True).start()
        if os.environ.get("ACP_REPLY_BACKPRESSURE") == "1":
            wait_for(f"drain-input-{number}")
    elif method == "session/cancel":
        if active is not None:
            active.set()
        if os.environ.get("ACP_CANCEL_UPDATE") == "1":
            send({"method": "session/update", "params": {
                "sessionId": "native-id",
                "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": " after cancel"}}
            }})
    elif method == "session/close":
        if os.environ.get("ACP_CLOSE_FAIL") == "1":
            send({"id": request["id"], "error": {"code": -32603, "message": "Close failed"}})
        else:
            response(request, {})
            if os.environ.get("ACP_UPDATE_AFTER_CLOSE") == "1":
                send({"method": "session/update", "params": {
                    "sessionId": "native-id", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "late output"}}
                }})
            if os.environ.get("ACP_EXIT_AFTER_CLOSE") == "1":
                os._exit(0)
    else:
        send({"id": request["id"], "error": {"code": -32601, "message": "Method not found"}})

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

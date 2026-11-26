"""Controlled stdio peer. All files live beneath the caller's temporary root."""
import json
import os
from pathlib import Path
import sys
import subprocess
import threading
import time

root = Path(os.environ["ACP_FIXTURE_ROOT"])
# A failed test still owns and removes its unique root; do not leave its detached fixture alive.
def watch_root():
    while root.exists():
        time.sleep(0.1)
    os._exit(0)

threading.Thread(target=watch_root, daemon=True).start()
lock = threading.Lock()
active = None
number = 0
effort = "medium"
interjections = []
protocol = json.loads(Path(__file__).with_name("interjection-protocol.json").read_text())
native_id = f"native-{os.getpid()}" if os.environ.get("ACP_UNIQUE_ID") == "1" else "native-id"


def send(message):
    with lock:
        print(json.dumps({"jsonrpc": "2.0", **message}), flush=True)


def log(message):
    with lock:
        with (root / "wire.jsonl").open("a") as output:
            output.write(json.dumps(message) + "\n")


def remember(update):
    with lock:
        with (root / "history.jsonl").open("a") as output:
            output.write(json.dumps(update) + "\n")


def response(request, result):
    send({"id": request["id"], "result": result})


def model_config():
    if not os.environ.get("ACP_MODELS"):
        return {}
    current = root / "model.txt"
    values = [
        {"value": "fast", "name": "Fast", "description": "Fast model"},
        {"value": "deep", "name": "Deep"},
    ]
    if os.environ.get("ACP_PROBE_FAILURES"):
        values[1:1] = [{"value": value, "name": value} for value in ("stalled", "rejected", "ignored")]
    if os.environ.get("ACP_MODELS") == "grouped":
        values = [{"group": "provider", "name": "Provider", "options": values}]
    options = [
        {"id": "behavior", "name": "Mode", "category": "mode", "type": "select",
         "currentValue": "ask", "options": [{"value": "ask", "name": "Ask"}]},
        {"id": "llm-selector", "name": "Model", "category": "model", "type": "select",
         "currentValue": current.read_text() if current.exists() else "fast", "options": values},
    ]
    if os.environ.get("ACP_EFFORT"):
        model = options[1]["currentValue"]
        options.append({"id": f"{model}-effort", "name": "Effort", "category": "thought_level", "type": "select",
                        "currentValue": effort, "options": [{"group": "levels", "name": "Levels", "options": [
                            {"value": "medium", "name": "Medium"}, {"value": "high", "name": "High", "description": "More thinking"},
                            {"value": "low", "name": "Low"}]}]})
        if model == "deep" and os.environ.get("ACP_MODEL_EFFORT"):
            options[-1].update(currentValue="max", options=[{"value": "max", "name": "Maximum"}])
    result = {"configOptions": options}
    if os.environ.get("ACP_GROK_MODELS"):
        result["models"] = {"currentModelId": "fast", "availableModels": [
            {"modelId": "deep", "name": "Deep", "_meta": {"supportsReasoningEffort": True,
             "reasoningEffort": "max", "reasoningEfforts": [{"value": "low", "default": True},
             {"id": "max", "label": "Maximum", "description": "Thinks longer"}]}},
            {"modelId": "fast", "name": "Fast", "_meta": {"supportsReasoningEffort": False}},
        ]}
    return result


def session_config(request, fields):
    if os.environ.get("ACP_MODELS") == "deferred":
        initial = []
        if os.environ.get("ACP_EFFORT"):
            model = model_config()["configOptions"][1]
            initial = [{**model, "options": [{"value": model["currentValue"], "name": "Current"}]}]
        response(request, {**fields, "configOptions": initial})
        send({"method": "session/update", "params": {"sessionId": native_id, "update": {
            "sessionUpdate": "config_option_update", **model_config()
        }}})
    else:
        options = {} if request["method"] == "session/load" and os.environ.get("ACP_MODELS") == "new-only" else model_config()
        response(request, {**fields, **options})


def wait_for(name):
    while not (root / name).exists():
        time.sleep(0.005)


def prompt(request, ordinal, cancellation):
    global active
    wait_for(f"update-{ordinal}")
    if os.environ.get("ACP_STANDARD_COMPACTION") == "1" and ordinal == 1:
        for update in [
            {"sessionUpdate": "compaction_update", "compactionId": "standard-one", "status": "in_progress"},
            {"sessionUpdate": "compaction_summary_chunk", "compactionId": "standard-one",
             "content": {"type": "text", "text": "retained context"}},
            {"sessionUpdate": "compaction_update", "compactionId": "standard-one", "status": "completed"},
        ]:
            send({"method": "session/update", "params": {"sessionId": native_id, "update": update}})
    if os.environ.get("ACP_READ_SESSION_ENV") == "1":
        value = subprocess.check_output(["sh", "-c", 'printf %s "$PONTIA_SESSION_ID"'], text=True)
        log({"session_environment": value, "runtime": os.environ.get("PONTIA_RUNTIME_INSTANCE_ID"), "native": native_id, "test_marker": os.environ.get("ACP_SESSION_MARKER")})
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
                "sessionId": "other-session" if os.environ.get("ACP_WRONG_SESSION") == "1" else native_id,
                "toolCall": {"toolCallId": "same-tool", "title": f"<script>untrusted</script> request:{request_id}", "rawInput": {"secret": "do-not-persist"}},
                "options": options,
            }})
    if os.environ.get("ACP_NATIVE_CANCEL") == "1":
        wait_for(f"native-cancel-{ordinal}")
        send({"method": "$/cancel_request", "params": {"requestId": f"unanswered-{ordinal}"}})
    if os.environ.get("ACP_EXTENSION_UPDATE") == "1":
        send({"method": "session/update", "params": {
            "sessionId": native_id, "update": {"sessionUpdate": "extension_running"}
        }})
        send({"id": "extension-barrier", "method": "_fixture_barrier", "params": {}})
    if os.environ.get("ACP_LIVE_SCRIPT") == "1":
        steps = json.loads((root / "live-script.json").read_text())
        for index, updates in enumerate(steps):
            wait_for(f"live-{ordinal}-{index}")
            for params in updates:
                if params.get("sessionId", native_id) == native_id:
                    remember(params["update"])
                send({"method": "session/update", "params": {"sessionId": native_id, **params}})
            send({"id": f"live-barrier-{ordinal}-{index}", "method": "_fixture_barrier", "params": {}})
    if os.environ.get("ACP_NO_UPDATES") != "1" and not (os.environ.get("ACP_SILENT_FIRST") == "1" and ordinal == 1):
        update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "streamed answer"}}
        remember(update)
        send({"method": "session/update", "params": {"sessionId": native_id, "update": update}})
    if os.environ.get("ACP_REPLY_BACKPRESSURE") == "1":
        wait_for(f"backpressure-{ordinal}")
        if os.environ.get("ACP_BREAK_REPLY") == "1":
            os._exit(0)
        # The main thread cannot drain stdin until these updates have all been
        # written. A response writer holding the reader's state lock deadlocks.
        for _ in range(64):
            send({"method": "session/update", "params": {
                "sessionId": native_id, "update": {
                    "sessionUpdate": "tool_call_update", "toolCallId": "pressure", "rawInput": "x" * 4096
                }
            }})
        if os.environ.get("ACP_RESULT_BEFORE_DRAIN") == "1":
            active = None
            response(request, {"stopReason": "end_turn"})
            wait_for(f"drain-result-{ordinal}")
            (root / f"drain-input-{ordinal}").touch()
            return
        (root / f"drain-input-{ordinal}").touch()
    wait_for(f"release-{ordinal}")
    if os.environ.get("ACP_DROP_PROMPT") == "1":
        os._exit(0)
    reason = "cancelled" if cancellation.is_set() else os.environ.get("ACP_STOP_REASON", request["params"]["prompt"][0]["text"])
    active = None
    if os.environ.get("ACP_INTERJECT") == "promote" and interjections:
        followups = list(interjections)
        interjections.clear()
        for i, text in enumerate(followups):
            native = f"interject-fallback-{ordinal}-{i}"
            send({"method":"_x.ai/queue/changed","params":{"sessionId":native_id,"runningText":text,"runningPromptId":native,"runningKind":"prompt","entries":[]}})
            if i == 0:
                response(request, {"stopReason":reason})
            remember({"sessionUpdate":"user_message_chunk","_meta":{"promptIndex":ordinal+i},"content":{"type":"text","text":text}})
            update = {"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":f"followup {i}"}}
            remember(update)
            send({"method":"session/update","params":{"sessionId":native_id,"_meta":{"promptId":native},"update":update}})
            send({"method":"_x.ai/session_notification","params":{"sessionId":native_id,"update":{"sessionUpdate":"turn_completed","prompt_id":native,"stop_reason":"end_turn"}}})
        return
    if os.environ.get("ACP_PROMPT_ERROR") == "1":
        send({"id": request["id"], "error": {"code": -32603, "message": "Native prompt failed: model unavailable"}})
        return
    response(request, {"stopReason": reason})
    if os.environ.get("ACP_RESULT_MARKER") == "1":
        (root / f"result-written-{ordinal}").touch()
    if os.environ.get("ACP_EXIT_AFTER_RESULT") == "1":
        os._exit(0)


log({"startup": {"cwd": os.getcwd(), "pid": os.getpid(), "session": os.environ.get("PONTIA_SESSION_ID"), "executable": sys.executable, "argv": sys.argv[1:], "literal": os.environ.get("ACP_LITERAL")}})
for line in sys.stdin:
    request = json.loads(line)
    log(request)
    method = request.get("method")
    if method is None:
        continue
    if method == "initialize":
        caps = {"loadSession": os.environ.get("ACP_NO_LOAD") != "1" and not (root / "no-load").exists(), "sessionCapabilities": {}}
        if os.environ.get("ACP_NO_CLOSE") != "1":
            caps["sessionCapabilities"]["close"] = {}
        if (os.environ.get("ACP_SYSTEM_PROMPT") == "1" or (root / "system-prompt").exists()) and not (root / "no-system-prompt").exists():
            caps["sessionCapabilities"]["systemPrompt"] = {}
        if os.environ.get("ACP_ANTIGRAVITY") == "1":
            caps["_meta"] = {"antigravity": {"systemPrompt": {"append": True}}}
        if os.environ.get("ACP_MODELS") == "deferred":
            caps["_meta"] = {"kiro": {}}
        if os.environ.get("ACP_DELETE"):
            caps["sessionCapabilities"]["delete"] = {}
        if os.environ.get("ACP_OPTIONS"):
            caps["sessionCapabilities"]["options"] = {}
        if os.environ.get("ACP_GROK_MODELS"):
            caps["_meta"] = {"x.ai/capabilities": {}}
        if os.environ.get("ACP_INTERJECT"):
            caps = protocol["initialize"]
        response(request, {"protocolVersion": 1, "agentCapabilities": caps,
                           **({"_meta": {"steering": {"supported": os.environ["ACP_STEERING"] == "1"}}} if "ACP_STEERING" in os.environ else {}),
                           "agentInfo": {"name": "fixture", "title": "Test Agent", "version": "arbitrary-release", "_meta": {"build": "fixture-build"}}})
    elif method == "_x.ai/models/list":
        kind = (root / "model-kind").read_text() if (root / "model-kind").exists() else "grok-build-plan"
        response(request, {"result": {"currentModelId": "fast", "availableModels": [
            {"modelId": "fast", "_meta": {"agentType": kind}},
            {"modelId": "deep", "_meta": {"agentType": "codex" if (root / "strict-deep").exists() else kind}},
            {"modelId": "unclassified"},
        ]}})
    elif method == "_x.ai/session/info":
        profile = "other" if (root / "wrong-profile").exists() else "pontia-restricted"
        model = (root / "model.txt").read_text() if (root / "model.txt").exists() else "fast"
        response(request, {"result": {"agentName": profile, "model": model}})
    elif method == "session/options":
        if os.environ.get("ACP_OPTIONS") == "error":
            send({"id": request["id"], "error": {"code": -32000, "message": "Authentication required"}})
        elif os.environ.get("ACP_OPTIONS") == "hold":
            wait_for("release-options")
        else:
            response(request, model_config())
    elif method == "session/new":
        if os.environ.get("ACP_REJECT_SYSTEM_PROMPT") == "new" and request["params"].get("systemPrompt"):
            send({"id": request["id"], "error": {"code": -32602, "message": "Instructions rejected"}})
            continue
        if os.environ.get("ACP_HOLD_NEW") == "1":
            wait_for("release-new")
        session_config(request, {"sessionId": native_id})
    elif method == "session/load":
        if os.environ.get("ACP_REJECT_SYSTEM_PROMPT") == "load" and request["params"].get("systemPrompt"):
            send({"id": request["id"], "error": {"code": -32602, "message": "Instructions rejected"}})
            continue
        gate = os.environ.get("ACP_LOAD_GATE")
        if gate and (root / gate).exists():
            wait_for(f"release-{gate}")
        if os.environ.get("PONTIA_SESSION_ID") and (root / f"hold-load-{os.environ['PONTIA_SESSION_ID']}").exists():
            wait_for(f"release-load-{os.environ['PONTIA_SESSION_ID']}")
        if (root / "hold-load").exists():
            wait_for("release-load")
        if (root / "load-fail").exists() or os.environ.get("ACP_LOAD_FAIL") == "1":
            send({"id": request["id"], "error": {"code": -32602, "message": "Native session unavailable"}})
            continue
        if request["params"]["sessionId"] != native_id:
            send({"id": request["id"], "error": {"code": -32602, "message": "Wrong native identity"}})
            continue
        if os.environ.get("ACP_COMPACT"):
            commands = [] if (root / "remove-compact").exists() else [{"name": "compact", "description": "Compress context"}]
            send({"method": "session/update", "params": {"sessionId": native_id, "update": {
                "sessionUpdate": "available_commands_update", "availableCommands": commands}}})
        history = root / "history.jsonl"
        if history.exists():
            for update in history.read_text().splitlines():
                native = "wrong-native-id" if (root / "wrong-replay").exists() else native_id
                update = json.loads(update)
                if update.get("sessionUpdate") == "compaction_checkpoint":
                    send({"method": "_x.ai/session/update", "params": {"sessionId": native, "update": update, "_meta": {"isReplay": True}}})
                elif os.environ.get("ACP_RECHUNK") == "1":
                    for character in update["content"]["text"]:
                        send({"method": "session/update", "params": {"sessionId": native, "update": {**update, "content": {"type": "text", "text": character}}}})
                else:
                    send({"method": "session/update", "params": {"sessionId": native, "update": update}})
        if (root / "orphan-replay").exists():
            send({"method": "session/update", "params": {"sessionId": native_id, "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "orphan answer"}}}})
        session_config(request, {})
        if os.environ.get("ACP_PAUSE_INPUT_AFTER_LOAD") == "1":
            (root / "input-paused").touch()
            wait_for("resume-input")
        if os.environ.get("ACP_BREAK_INPUT") == "1":
            sys.stdin.read(1)
            os._exit(0)
    elif method == "_x.ai/compact_conversation" and os.environ.get("ACP_COMPACT"):
        wait_for("release-compact")
        if (root / "reject-compact").exists():
            send({"id": request["id"], "error": {"code": -32000, "message": "Compaction rejected"}})
            continue
        remember({"sessionUpdate": "compaction_checkpoint", "checkpoint_id": "cp-one",
                  "created_at": "2026-11-23T00:00:00Z"})
        send({"method": "_x.ai/session_notification", "params": {"sessionId": native_id,
              "_meta": {"eventId": "compact-one"}, "update": {"sessionUpdate": "auto_compact_completed",
              "tokens_before": 1024, "tokens_after": 1024}}})
        response(request, {})
    elif method == "session/set_config_option":
        params = request["params"]
        if os.environ.get("ACP_EFFORT") and params["configId"] == model_config()["configOptions"][-1]["id"]:
            effort = os.environ.get("ACP_EFFECTIVE_EFFORT", params["value"])
            response(request, model_config())
            continue
        if params["value"] == "stalled":
            continue
        if params["value"] == "ignored":
            response(request, model_config())
            continue
        if (os.environ.get("ACP_REJECT_MODEL") == "1" or params["sessionId"] != native_id
                or params["configId"] != "llm-selector" or params["value"] not in ["fast", "deep"]):
            send({"id": request["id"], "error": {"code": -32602, "message": "Model rejected"}})
            continue
        (root / "model.txt").write_text(os.environ.get("ACP_MODEL_OVERRIDE", params["value"]))
        response(request, model_config())
        if os.environ.get("ACP_MODEL_AFTER_SET") == "1":
            (root / "model.txt").write_text("fast")
            send({"method": "session/update", "params": {"sessionId": native_id,
                  "update": {"sessionUpdate": "config_option_update", **model_config()}}})
            send({"id": "model-barrier", "method": "_fixture_barrier", "params": {}})
    elif method == "session/prompt":
        if os.environ.get("ACP_MODELS"):
            log({"prompt_model": model_config()["configOptions"][1]["currentValue"]})
        initial = {"sessionUpdate": "user_message_chunk", "content": request["params"]["prompt"][0]}
        if os.environ.get("ACP_INTERJECT"):
            initial["_meta"] = {"promptIndex":number}
            send({"method":"_x.ai/queue/changed","params":{"sessionId":native_id,"runningText":initial["content"]["text"],"runningPromptId":f"initial-{number}","runningKind":"prompt","entries":[]}})
        remember(initial)
        if active is not None:
            log({"overlap": True})
        number += 1
        active = threading.Event()
        threading.Thread(target=prompt, args=(request, number, active), daemon=True).start()
        if os.environ.get("ACP_REPLY_BACKPRESSURE") == "1":
            wait_for(f"drain-input-{number}")
    elif method == "_x.ai/interject" and os.environ.get("ACP_INTERJECT") == "unsupported":
        send({"id":request["id"],"error":protocol["method_not_found"]})
    elif method == "_x.ai/interject":
        params = request["params"]
        echo = {"method":"_x.ai/session/interjection","params":params}
        mode = os.environ["ACP_INTERJECT"]
        if mode == "disconnect":
            os._exit(0)
        if mode == "unknown":
            response(request, {"result":{"status":"unrecognized"}})
            continue
        if mode == "echo-first":
            send(echo)
        response(request, protocol["receipt"])
        send(echo)
        if mode == "promote":
            interjections.append(params["text"])
        else:
            remember({"sessionUpdate":"user_message_chunk","_meta":{"interjection":True},"content":{
                "type":"text","text":"<user_query>"+params["text"]+"</user_query>","_meta":{"displayText":params["text"]}}})
    elif method == "_session/steer" and os.environ.get("ACP_KIRO_STEER"):
        if not request["params"]:
            send({"id": request["id"], "error": {"code": -32603, "message": "Internal error",
                  "data": {"details": "_session/steer: missing or invalid sessionId"}}})
            continue
        message_id = "steer-" + str(request["id"])
        text = request["params"]["message"]
        response(request, {"queued": True, "messageId": message_id})
        def injected(sid, mid, replay=False):
            send({"method": "session/update", "params": {"sessionId": sid, "update": {
                "sessionUpdate": "session_info_update", "_meta": {"kiro": {
                    "kind": "steering_injected", "messageId": mid, "replay": replay}}}}})
        injected(native_id, "unrelated")
        injected("other-session", message_id)
        injected(native_id, message_id, True)
        send({"id": "steer-queued-barrier", "method": "_fixture_barrier", "params": {}})
        def inject_steer(message_id, text):
            if os.environ["ACP_KIRO_STEER"] in ("inject", "reattach"):
                wait_for("inject")
                injected(native_id, message_id)
                remember({"sessionUpdate": "user_message_chunk", "_meta": {"kiro": {
                    "source": "steer", "messageId": message_id}}, "content": {"type": "text", "text": text}})
                for chunk in ["前[ST", "EERING " + message_id, ": note", "]後[OTHER]"]:
                    if chunk.startswith("]") and os.environ["ACP_KIRO_STEER"] == "reattach":
                        send({"id": "note-barrier", "method": "_fixture_barrier", "params": {}})
                        wait_for("continue-note")
                    update = {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": chunk}}
                    send({"method": "session/update", "params": {"sessionId": native_id, "update": update}})
                    remember(update)
            send({"id": "steer-barrier", "method": "_fixture_barrier", "params": {}})
        threading.Thread(target=inject_steer, args=(message_id,text), daemon=True).start()
    elif method == "_session/steering":
        outcome = os.environ.get("ACP_STEERING_OUTCOME", "injected")
        if outcome == "disconnect":
            os._exit(0)
        if outcome == "injected":
            remember({"sessionUpdate": "user_message_chunk", "content": request["params"]["prompt"][0]})
        response(request, {"outcome": outcome, **({"reason": "noRunningTurn"} if outcome == "promptRequired" else {})})
    elif method == "session/cancel":
        if active is not None:
            active.set()
        if os.environ.get("ACP_CANCEL_UPDATE") == "1":
            send({"method": "session/update", "params": {
                "sessionId": native_id,
                "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": " after cancel"}}
            }})
    elif method == "session/delete" and os.environ.get("ACP_DELETE") and request["params"]["sessionId"] == native_id:
        response(request, {})
    elif method == "session/close":
        if os.environ.get("ACP_HOLD_CLOSE") == "1":
            wait_for("release-close")
        if os.environ.get("ACP_DROP_CLOSE") == "1":
            os._exit(0)
        if os.environ.get("ACP_CLOSE_FAIL") == "1" or (root / "close-fail").exists():
            send({"id": request["id"], "error": {"code": -32603, "message": "Close failed"}})
        else:
            response(request, {})
            if os.environ.get("ACP_UPDATE_AFTER_CLOSE") == "1":
                send({"method": "session/update", "params": {
                    "sessionId": native_id, "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "late output"}}
                }})
            if os.environ.get("ACP_EXIT_AFTER_CLOSE") == "1":
                os._exit(0)
    elif "id" in request:
        send({"id": request["id"], "error": {"code": -32601, "message": "Method not found"}})

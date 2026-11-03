import http.server
import json
import uuid


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        inputs = request.get("input", [])
        last_user = max(
            (i for i, item in enumerate(inputs) if item.get("role") == "user"),
            default=-1,
        )
        completed = any(
            item.get("type") == "function_call_output"
            for item in inputs[last_user + 1:]
        )
        if completed:
            item = {
                "type": "message", "id": "msg_" + uuid.uuid4().hex,
                "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": "Done.", "annotations": []}],
            }
        else:
            command = "python3 -c 'import os,json; print(json.dumps({k: os.environ.get(k) for k in [\"PONTIA_SESSION_ID\",\"PROBE_KEEP\",\"PROBE_DROP\",\"PROBE_NATIVE\"]}))'"
            tools = {tool.get("name") for tool in request["tools"]}
            name = "exec_command" if "exec_command" in tools else "shell_command"
            arguments = {"cmd" if name == "exec_command" else "command": command, "login": False}
            item = {
                "type": "function_call", "id": "fc_" + uuid.uuid4().hex,
                "call_id": "call_" + uuid.uuid4().hex, "name": name,
                "arguments": json.dumps(arguments),
                "status": "completed",
            }
        response_id = "resp_" + uuid.uuid4().hex
        events = [
            {"type": "response.created", "response": {"id": response_id, "status": "in_progress", "output": []}},
            {"type": "response.output_item.added", "output_index": 0, "item": item},
            {"type": "response.output_item.done", "output_index": 0, "item": item},
            {"type": "response.completed", "response": {
                "id": response_id, "status": "completed", "output": [item],
                "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
            }},
        ]
        data = "".join(
            "event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n"
            for event in events
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print(server.server_port, flush=True)
server.serve_forever()

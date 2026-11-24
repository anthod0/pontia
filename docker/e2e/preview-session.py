"""Prepare a real Session and file for the public Dashboard test, inside the device."""

import json
from pathlib import Path
import time
import tomllib
import urllib.request
import uuid

config = tomllib.loads(Path("/home/e2e/.pontia/config.toml").read_text())


def request(path, payload):
    req = urllib.request.Request("http://127.0.0.1:8080/api/v1/" + path,
                                 data=json.dumps(payload).encode(),
                                 headers={"Authorization": "Bearer " + config["external_api_token"],
                                          "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as response:
        return json.load(response)["data"]


workspace = request("workspaces", {"root_id": "e2e"})["workspace"]["canonical_path"]
marker = uuid.uuid4().hex
file = Path(workspace) / f"preview-{marker}.txt"
ports = [5173, 5174, 5175]
links = [f"[Preview {port}](http://localhost:{port}/page?a=1#sec)" for port in ports]
links.append(f"[Result]({file})")
task = (f"Use Bash to write the single line {marker} to {file}. "
        "Then reply with exactly these four Markdown links, outside code blocks:\n" + "\n\n".join(links))
session = request("sessions", {
    "workspace": workspace, "client_type": "claude-agent", "model": "gpt-4o-mini", "mode": "yolo",
    "initial_task": {"input": task},
})["session"]["session_id"]
for attempt in range(120):
    if file.exists() and file.read_text().strip() == marker:
        break
    time.sleep(1)
else:
    raise RuntimeError(f"Session {session} did not write its result")
print(json.dumps({"session": session, "device": config["remote"]["device_id"], "marker": marker}))

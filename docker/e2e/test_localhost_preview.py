"""Run on a fresh instance with scripts/docker-e2e run python3 docker/e2e/test_localhost_preview.py."""

import json
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
LAB = str(HERE.parents[1] / "scripts/docker-e2e")


def execute(service, *args, **kwargs):
    return subprocess.run([LAB, "compose", "exec", "-T", service, *args], check=True, **kwargs)


def register():
    approve = """
        const { chromium } = require('/opt/browser/node_modules/playwright-core');
        (async () => {
          const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
          const page = await browser.contexts()[0].newPage();
          await page.goto(process.argv[1]);
          await page.locator('button[formaction="?/approve"]').click();
          await browser.close();
        })();
    """
    with subprocess.Popen([LAB, "login-device"], stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True) as login:
        dashboard = None
        for line in login.stdout:
            verification = re.search(r"https://pontia\.dev/device\?user_code=[A-Za-z0-9-]+", line)
            if verification:
                execute("browser", "node", "-e", approve, verification[0], timeout=45)
            match = re.search(r"https://app\.pontia\.dev/[\w-]+", line)
            if match:
                dashboard = match[0]
        if login.wait() or not dashboard:
            raise RuntimeError("Test device registration failed")
        return dashboard


def main():
    dashboard = register()
    for service, name in (("device", "preview-service.mjs"), ("browser", "localhost-preview.mjs")):
        subprocess.run([LAB, "compose", "cp", str(HERE / name), f"{service}:/tmp/{name}"], check=True)
    subprocess.run([LAB, "compose", "exec", "-d", "device", "sh", "-c",
                    "exec bun /tmp/preview-service.mjs > /tmp/preview-requests.jsonl"], check=True)
    prepared = execute("device", "python3", "-", input=(HERE / "preview-session.py").read_bytes(),
                       stdout=subprocess.PIPE, timeout=180)
    print("Dashboard Session", json.loads(prepared.stdout)["session"], flush=True)
    execute("browser", "node", "/tmp/localhost-preview.mjs", dashboard,
            input=prepared.stdout, timeout=180)
    log = execute("device", "cat", "/tmp/preview-requests.jsonl", stdout=subprocess.PIPE)
    requests = [json.loads(line) for line in log.stdout.splitlines()]
    assert all(not row["previewCookie"] for row in requests)
    for family in ("127.0.0.1", "::1"):
        paths = {row["path"] for row in requests if row["hostname"] == family and row["appCookie"]}
        assert {"/redirect", "/after", "/events", "/ack", "/hmr"} <= paths
        print(f"PASS device log {family}: application cookie received, preview cookie absent")


if __name__ == "__main__":
    main()
